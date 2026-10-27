/// Buffer bytes until a complete line exists: network frames may split a UTF-8 character.
#[derive(Default)]
pub struct SseDecoder {
    pending: Vec<u8>,
    data: Vec<String>,
    event_bytes: usize,
}
impl SseDecoder {
    pub fn push(&mut self, bytes: &[u8]) -> Result<Vec<String>, String> {
        self.pending.extend_from_slice(bytes);
        if self.pending.len() > 4 * 1024 * 1024 {
            return Err("流式事件超过大小限制。".into());
        }
        let mut events = Vec::new();
        let mut consumed = 0;
        while let Some(offset) = self.pending[consumed..]
            .iter()
            .position(|byte| *byte == b'\n')
        {
            let end = consumed + offset;
            let line = std::str::from_utf8(&self.pending[consumed..end])
                .map_err(|_| "服务返回了无效的 UTF-8 内容。".to_string())?
                .trim_end_matches('\r');
            if line.is_empty() {
                if !self.data.is_empty() {
                    events.push(self.data.join("\n"));
                    self.data.clear();
                    self.event_bytes = 0;
                }
            } else if let Some(data) = line.strip_prefix("data:") {
                let data = data.strip_prefix(' ').unwrap_or(data);
                self.event_bytes += data.len();
                if self.event_bytes > 4 * 1024 * 1024 {
                    return Err("流式事件超过大小限制。".into());
                }
                self.data.push(data.to_owned());
            }
            consumed = end + 1;
        }
        self.pending.drain(..consumed);
        Ok(events)
    }
    pub fn finish(&mut self) -> Result<Vec<String>, String> {
        // Some compatible servers omit the final blank line.
        self.push(b"\n\n")
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preserves_unicode_across_every_possible_chunk_boundary() {
        let stream =
            ": heartbeat\r\nevent: delta\r\ndata: {\"text\":\"你好 🌱\"}\r\n\r\ndata: [DONE]\n\n";
        for split in 0..stream.len() {
            let mut decoder = SseDecoder::default();
            let mut events = decoder.push(&stream.as_bytes()[..split]).unwrap();
            events.extend(decoder.push(&stream.as_bytes()[split..]).unwrap());
            assert_eq!(events, vec!["{\"text\":\"你好 🌱\"}", "[DONE]"]);
        }
    }
    #[test]
    fn handles_multiline_data_and_missing_final_delimiter() {
        let mut decoder = SseDecoder::default();
        assert!(
            decoder
                .push(b"data: first\ndata:second")
                .unwrap()
                .is_empty()
        );
        assert_eq!(decoder.finish().unwrap(), vec!["first\nsecond"]);
    }
}
