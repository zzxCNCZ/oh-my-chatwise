/** WebKit desktop webviews may not resolve navigator.clipboard outside an active tab.
 * Keep the synchronous copy within the click gesture, with the modern API as fallback.
 */
export async function copyText(text: string) {
  const active = document.activeElement instanceof HTMLElement ? document.activeElement : null;
  const selection = window.getSelection();
  const ranges = selection
    ? Array.from({ length: selection.rangeCount }, (_, i) => selection.getRangeAt(i).cloneRange())
    : [];
  const field = document.createElement('textarea');
  field.value = text;
  field.readOnly = true;
  field.style.cssText = 'position:fixed;left:-9999px;top:0;opacity:0;pointer-events:none';
  field.setAttribute('aria-hidden', 'true');
  document.body.append(field);
  let copied = false;
  try {
    field.focus({ preventScroll: true });
    field.select();
    copied = document.execCommand('copy');
  } catch {
    /* Use the modern clipboard API below. */
  } finally {
    field.remove();
    active?.focus({ preventScroll: true });
    if (selection && ranges.length) {
      selection.removeAllRanges();
      for (const range of ranges) selection.addRange(range);
    }
  }
  if (!copied) {
    if (!navigator.clipboard) throw new Error('剪贴板不可用');
    await navigator.clipboard.writeText(text);
  }
}
