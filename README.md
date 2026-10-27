# oh-my-chatwise

使用 **Tauri 2 + Vue 3 + TypeScript + Vite + Rust** 构建的本地桌面 AI 聊天应用，面向 macOS 和 Windows。界面采用统一的绿色品牌、浅色磨砂双栏布局，并提供同体系深色模式，不预置供应商、API 地址、密钥或模型列表。模型列表从用户配置的 API 动态获取。

## 设计语言与品牌

全项目统一使用 Forest Glass 设计语言：浅色磨砂、薄雾绿导航、绿色渐变主操作与柔和圆角。深色模式沿用同一套绿色品牌和层级，已有主题选择保留。完整规范见 [design/DESIGN_SYSTEM.md](design/DESIGN_SYSTEM.md)，后续界面按此规范延续。

用户提供的 Logo 原图保存在 `design/reference/logo-sheet.png`，切片在 `public/brand/`；通过 `python3 scripts/slice-brand.py` 和 `pnpm tauri icon design/app-icon-source.png` 可重新生成切片与系统图标。

## 运行

环境要求：Node.js 24、pnpm 11、Rust stable，以及当前系统的 [Tauri 开发依赖](https://v2.tauri.app/start/prerequisites/)。macOS 需要 Xcode Command Line Tools；Windows 需要 MSVC C++ Build Tools 和 WebView2。

```sh
pnpm install
pnpm tauri dev
```

仅预览前端：

```sh
pnpm dev
```

浏览器预览使用独立的 localStorage，支持界面和会话管理，不保存密钥、不发送模型请求。真实请求与凭据存储操作由 Tauri/Rust 提供。

## 连接模型

打开左下角工作空间设置，在「模型连接」添加连接，填写名称、API 协议、基础地址、API 密钥（可选），然后点击「获取模型」。在返回的列表中搜索、勾选并添加需要的模型，再保存连接。已配置的模型以表格展示，可修改显示名称、移除单个模型或清空后重新选择；顶部模型选择器使用显示名称，实际请求仍发送原始模型 ID。

| 协议                    | 基础地址示例                  | 追加的路径          | 认证                                          |
| ----------------------- | ----------------------------- | ------------------- | --------------------------------------------- |
| OpenAI Chat Completions | `https://your-api.example/v1` | `/chat/completions` | `Authorization: Bearer …`                     |
| OpenAI Responses        | `https://your-api.example/v1` | `/responses`        | `Authorization: Bearer …`                     |
| Anthropic Messages      | `https://your-api.example/v1` | `/messages`         | `x-api-key` + `anthropic-version: 2023-06-01` |

- 基础地址必须包含服务要求的版本前缀；也支持填写完整端点，不会重复拼接路径。不自动添加 `/v1`。
- 地址示例只是说明，并非内置服务。支持自定义网关、兼容接口和本地 HTTP 服务。
- 获取模型通过 `GET {基础地址}/models` 完成，也能从完整聊天端点推导模型列表地址。OpenAI 两种协议使用 Bearer 认证，Anthropic 使用 `x-api-key` 与版本头并自动读取分页。获取时优先使用当前输入的密钥，未输入时使用该连接已保存的密钥，无须先保存连接。
- 接口失败、认证失败、未提供 `/models` 或返回空列表会显示具体提示，保留已配置模型。API 列出的模型可能包含图像、语音等非文本模型，请按当前应用的文本对话能力选择。
- 留空密钥可连接无需认证的本地服务；编辑现有连接时留空会保留原密钥。删除连接会一并删除密钥。
- Chat Completions 使用 `max_completion_tokens`、`stream_options.include_usage`，兼容服务也需支持这些标准字段。
- Responses 使用 `max_output_tokens`、`instructions` 与 `store: false`。Anthropic 使用 `max_tokens` 与独立 `system` 字段。
- Temperature 默认不发送；在「设置 → 生成与工具限额」中统一配置。开启思考时不发送 Temperature。
- 请求携带当前会话历史；API 返回的文本以 SSE 流式显示，也兼容返回完整 JSON 的接口。不会自动重试付费请求。

协议实现参考 [OpenAI Chat Completions](https://developers.openai.com/api/reference/resources/chat/subresources/completions/streaming-events)、[OpenAI Responses 流式指南](https://developers.openai.com/api/docs/guides/streaming-responses) 和 [Anthropic 流式指南](https://platform.claude.com/docs/en/build-with-claude/streaming)。模型发现参考 [OpenAI 模型列表](https://developers.openai.com/api/reference/resources/models/methods/list) 与 [Anthropic 模型列表](https://platform.claude.com/docs/en/api/models/list)。

## 功能

- 会话新建、搜索、重命名、置顶、删除，后台使用 AI 总结开头对话生成标题，支持手动重命名。
- 流式文本、可折叠思考内容（服务返回时）、停止生成、错误提示与重新生成。
- Markdown、表格、常用语言代码高亮、代码/回复复制、Markdown 导出。
- 图片与文本附件分别添加，支持拖拽、多个附件、文字与图片同时发送，以及仅图片消息。
- PNG/JPEG/WebP 图片默认以 Base64 发送；也支持公网图片 URL 和自定义图床 / OSS 上传。
- 文本文件按格式解析（256 KB 以内），图片、附件与会话草稿自动保存。
- 每段会话独立选择连接、模型、提示词和 Web / HTTP / MCP 开关；生成参数与工具限额统一配置。
- 深色、浅色和跟随系统外观；窄窗口可折叠侧栏。
- 原生 macOS 标题栏，Windows 使用系统窗口控件。

快捷键：`⌘ / Ctrl + N` 新对话，`⌘ / Ctrl + K` 搜索，`⌘ / Ctrl + ,` 设置。`Enter` 发送，`Shift + Enter` 换行，兼容中文输入法选词。

当前实现支持文本与视觉模型输入，以及可配置的工具循环、HTTP 搜索接口与 MCP 工具。未实现语音、内置同步服务或内置模型供应商。图片理解能力取决于用户选择的模型。使用指南是明确标记的本地说明，不会调用模型。

## 会话标题

新对话先用首条消息作临时标题，首个完整回复后，后台使用当前连接及模型总结标题。第三次完整对话后再完善一次并定稿，之后不随话题变化继续改写。正常最多发起两次额外的短模型请求，失败只在开头三次完整对话内重试，总尝试上限三次；失败不影响聊天，保留原名。标题请求有 30 秒超时，不执行工具。

标题只使用开头三次成功对话的用户文本和最终回复，单段最多 2000 字符；附件只传名称，不传文件内容、图片、工具原始结果或思考内容。生成状态随会话保存及同步，重新打开客户端不会不断重新命名。

在会话菜单中随时「重命名」，手动修改会锁定自动命名，正在返回的 AI 标题也不会覆盖。旧会话保留原名；可通过菜单「AI 生成标题」按需重新总结，此操作也只参考开头对话。

## 提示词与思考强度

在「设置 → 提示词管理」中添加、编辑或删除提示词，并指定默认提示词。输入框可直接选择本轮系统提示词，每次回复前都可切换；不选或引用的提示词已删除时使用默认提示词。旧会话的自定义系统提示词自动迁入提示词库并保持选中。提示词按独立记录同步，可对并发修改保留冲突副本。

在「设置 → 模型连接」的已配置模型表格中选择思考强度，按模型保存，默认 **Medium**。支持关闭、Low、Medium、High、Xhigh、Max；OpenAI 还提供 Minimal。具体等级与关闭能力由模型 API 决定，不支持相关字段的服务可选择「不发送参数」。

- Chat Completions：`reasoning_effort`，关闭使用 `none`。
- Responses：`reasoning.effort`，关闭使用 `none`。
- Anthropic：`thinking.type: adaptive` 与 `output_config.effort`；关闭使用 `thinking.type: disabled`。需要支持自适应思考的模型；旧模型或兼容网关可选择「不发送参数」。
- 开启思考时不发送 Temperature，避免与思考参数冲突；输出上限包含思考与回答。工具续写的每一次模型请求使用相同的思考配置，不自动降级或重试。

协议参考 [OpenAI Reasoning](https://developers.openai.com/api/docs/guides/reasoning) 与 [Anthropic Effort](https://platform.claude.com/docs/en/build-with-claude/effort)。

## Agent 与工具循环

参考 `tmc-agent-kernel` 的循环、注册表、执行策略和限额机制，将核心能力迁移为 Rust 模块，不依赖 Python 运行时。在「设置 → 工具与 Agent」配置外部工具。会话的工具设置只包含 Web 搜索、自定义 HTTP、MCP 三类开关，默认开启；内置时间和计算工具在每个会话中始终可用。关闭某一类别后，后端不会将这类工具注册到该会话的模型请求中。支持 OpenAI Chat Completions、Responses 与 Anthropic Messages：模型提出工具调用 → 校验与授权 → 执行 → 将结果交回模型，直到产生最终回复或达到全局限制。模型必须支持工具调用。

### 三类工具

- **内置工具**：`current_time` 获取 UTC 时间，`calculate` 执行有限的四则运算、取模和括号表达式，不执行任意代码或系统命令。
- **HTTP 工具**：配置工具名称、描述、参数 JSON Schema、固定 HTTP(S) URL、方法、Header、Query、JSON Body 与返回值提取路径。Body / Query 的 `{{参数名}}` 占位符从模型参数替换；整个字符串都是占位符时保留 JSON 类型，嵌入其他文字时转为文本，不会递归展开。Header 与 URL 不接受模型模板。测试按钮会实际调用一次当前配置的只读接口，不调用模型。
- **MCP 工具**：添加 **Streamable HTTP** 服务 URL 与 Header，点击发现，再勾选工具并保存；刷新发现会保留已有勾选及风险设置。不同服务器的工具自动命名隔离，新发现工具默认关闭并要求逐次授权。当前不支持 stdio、旧版独立 SSE 传输、OAuth 登录、sampling 或 elicitation；可使用 Header 配置服务所需的认证。

**快速接入 Brave Search**：打开「设置 → 工具与 Agent → Web Search · Brave」，填写 API Key，点击「保存工具配置」。默认启用，API 地址、鉴权头和请求参数由应用内置；不需要编辑 JSON。保存配置后，Web 搜索开关已开启的会话即可调用 `brave_search`。每次默认返回 5 个结果，模型可指定 1–20 个；结果只保留标题、摘要与来源 URL。可以先点「测试 Brave Search」，它会用当前输入的 Key 执行一次「Brave Search」搜索，不调用模型，测试不会自动保存配置。Key 保存在数据目录的本地凭据记录中，可随工具配置同步及加密导出，不进入模型参数或聊天记录。清空 Key 并保存，或取消启用，即可停止注册该工具。Brave 接口参考[官方 Web Search 文档](https://api-dashboard.search.brave.com/app/documentation/web-search/get-started)。

其他 Web Search 服务仍可通过「自定义搜索」接入。通用 GET 搜索模板的 Query 为 `{"q":"{{query}}","format":"json"}`，填写兼容搜索服务的地址即可。其他搜索 API 可自行修改方法、Header、Query 和 Body；例如通用 JSON POST 可设为 `{"query":"{{query}}","limit":5}`，具体字段按所用服务文档配置。应用不内置搜索密钥，模型必须支持工具调用。

只读工具自动执行；「外部操作」工具在每次调用前显示目标地址与参数，允许一次或拒绝。风险类型由用户配置，不自动信任 MCP 服务返回的风险注解。工具结果是模型上下文中的外部数据，不应视作用户授权；授权只在当前请求内有效，不写入会话或同步文件。

### 工具本地代理

在「设置 → 工具与 Agent → 网络代理」打开「使用本地代理」，填写代理应用的实际地址与端口后保存工具配置。例如 HTTP 代理 `http://127.0.0.1:7890`，或 SOCKS5 代理 `socks5h://127.0.0.1:1080`。支持 HTTP、HTTPS、`socks5://` 和 `socks5h://`；推荐 SOCKS5H，让目标域名由代理端解析，避免本机 DNS 解析失败或污染。

此设置覆盖 Brave Search、自定义 HTTP 工具，以及 MCP 的发现和调用。测试按钮使用页面当前填写的代理配置，无须先保存；对话中的实际调用在保存后从下一次回复生效。开启后通过指定代理连接，代理失败不自动改为直连。关闭后沿用原有默认网络连接；模型 API、模型发现、标题生成和图床保持各自的网络行为。

可以用「测试 Brave Search」验证实际搜索链路（需要自己的 API Key），也可以测试已配置的 HTTP 接口或发现 MCP 工具。代理开关与地址作为工具配置的一部分保存、同步，并包含在「含凭据」的加密配置导出中；不会放入模型上下文。旧配置未设置代理时默认关闭。

参考 [reqwest Proxy](https://docs.rs/reqwest/latest/reqwest/struct.Proxy.html)。

### 排查未调用工具

点击输入框旁的「工具」或顶部「对话设置」，检查该会话的 Web 搜索、HTTP、MCP 开关；悬停工具按钮可查看当前可用的工具名。显示「工具配置读取失败」时，检查本机凭据存储与工具设置。旧会话明确关闭工具的状态会迁移为三类外部工具均关闭，内置时间和计算仍可用。

模型根据问题决定是否调用工具，可以直接回答无需工具的问题。需要检索时请明确提出搜索要求；实际执行会出现工具记录，并在结果返回后继续请求模型。模型只用文字声称搜索而没有工具记录，不代表执行了搜索。

### 限额、恢复与扩展

「设置 → 生成与工具限额」全局配置最大输出 Tokens、Temperature、模型请求轮数、工具总调用数、单工具总限额、单轮同名工具限额、工具超时和整次回复超时，所有会话从下一次回复使用新值。首次升级时采用升级前当前会话的生成参数和限额，旧字段保留用于数据兼容。默认 8 轮、16 次工具调用、单工具 30 秒、总计 300 秒。批次超过剩余总量时整批拒绝；同名工具超过该工具限额时整个同名组拒绝，避免只执行部分批次。参数校验失败、未知工具、执行失败和拒绝会作为结果交回模型，不会自动重试外部操作。Schema 校验覆盖基础类型、required、additionalProperties、enum、anyOf、items、maxItems、maxLength 和数值上下界，暂不实现完整 JSON Schema / `$ref`。

调用按模型返回顺序串行执行。停止会终止本地循环并阻止后续工具启动；已经发往远端的操作可能仍会完成，停止不能撤销外部操作。MCP 当前不保证远端取消或会话关闭。

聊天中以「正在搜索网页」「正在执行工具」等进度展示实际执行状态，不显示模型轮次；服务返回的思考内容与工具记录按顺序保留。开始生成回答时自动折叠，可手动展开查看思考、参数、结果与状态，Markdown 导出也包含工具记录。工具输出限制为 64 KB，每次回复的工具记录限制为 12 MB。工具调用和结果随会话保存；中断后保留记录并标记停止，不自动恢复执行。同一连接、模型和协议继续对话时，保留原始工具上下文，包括 Responses 的推理项与 Anthropic 的签名内容；切换模型后使用最终回复文本继续，避免混用协议内部记录。

HTTP / MCP 的完整配置含鉴权，存入数据目录的本地凭据记录，并随同步软件同步；可选启用凭据加密。含凭据的配置导出也包含工具配置，导入时不会自动连接或执行工具。不含凭据的导出排除这部分私有配置。会话中的工具参数与结果仍为普通 JSON；可识别的静态鉴权值会从外部结果中清理，工具返回内容应按其实际敏感程度管理。

扩展点位于 `src-tauri/src/agent/`：`ModelClient` 负责模型单轮、`Tool` 负责实际执行、`Registry` 显式注册工具、`ToolPolicy` 决定是否授权，`run` 统一控制循环、事件与限额。

协议参考 [OpenAI Function calling](https://developers.openai.com/api/docs/guides/function-calling)、[Anthropic 流式工具与思考内容](https://platform.claude.com/docs/en/build-with-claude/streaming) 与 [MCP Streamable HTTP](https://modelcontextprotocol.io/specification/2025-06-18/basic/transports)。

## 图片与图床

聊天输入框提供独立的「图片」「图片 URL」「文本」入口，也支持将图片和文本文件一起拖入聊天区域。附件会显示预览、文件类型、上传状态与错误；发送前可移除，失败的图床上传可重试。可只发送图片，或与文本、文本附件一起发送。

图片支持 PNG、JPEG、WebP，单张不超过 5 MB、每条最多 5 张、合计 20 MB。Base64 图片保存为本机 `images/<UUID>.bin` 文件，聊天 JSON 只保存引用。发送时由 Rust 读取文件，并按协议构建：

| 协议             | Base64 图片                                         | 公网 URL                            |
| ---------------- | --------------------------------------------------- | ----------------------------------- |
| Chat Completions | `image_url.url` 内的 `data:<mime>;base64,...`       | `image_url.url`                     |
| Responses        | `input_image.image_url` 内的 data URL               | `input_image.image_url`             |
| Anthropic        | `image.source = {type: "base64", media_type, data}` | `image.source = {type: "url", url}` |

所有历史图片引用会保留到后续请求上下文中；不会因切换全局图片模式而重新上传历史图片。已存在的公网图片 URL 可直接添加，无须经过图床。公网地址需要能被所选模型服务访问。

在「设置 → 图片与图床」中选择模式。默认 Base64 不需要图床；图床 URL 模式会先上传本地图片，上传成功后再将返回的 URL 加入模型消息。支持以下上传配置：

- 方法：POST / PUT / PATCH。
- Body：multipart/form-data、JSON、application/x-www-form-urlencoded、二进制文件（适用于 OSS 预签名 PUT）。
- 自定义 Header JSON、multipart 文件字段名、Body 参数 JSON，支持嵌套 JSON。
- Body 字符串模板：`{{base64}}`（不含 data 前缀）、`{{dataUrl}}`、`{{filename}}`、`{{mimeType}}`、`{{size}}`、`{{id}}`。
- 上传地址和公网 URL 模板支持 `{{id}}` 与 `{{filename}}`，文件名按 URL 编码处理。
- 返回 URL 提取：JSON 点路径 / 数组索引 / JSON Pointer、纯文本 URL、自定义 OSS/CDN 公网 URL 模板。
- 测试使用当前未保存配置上传一张 1×1 PNG，不调用模型；实际上传成功并提取到 HTTP(S) URL 才显示成功。

例：JSON 图床可以将 Body 设置为 `{"image":"{{base64}}"}`，响应路径设为 `data.url`。OSS 表单上传可将 policy、key、签名等配置在 Body 字段中，文件通过指定的 multipart 字段附带。OSS 预签名 PUT 使用二进制 Body，URL 模板应对应对象的公开读取地址。签名由用户的服务提供，应用不保存 OSS AccessKey、不内置 OSS 签名服务。

完整图床配置（包括 Header、Body、签名 URL）存入数据目录的本地凭据记录。聊天 JSON 不含图床鉴权配置。Base64 和图床上传都在桌面应用中工作，浏览器仅用于界面预览。删除聊天不会立即清理本机图片文件，以免破坏备份中的引用；本机数据目录需要与聊天记录一起管理。

## 文本附件解析

文本附件独立于图片处理，单个最多 256 KB，每条最多 10 个。支持 UTF-8 和带 BOM 的 UTF-16：

- TXT / LOG：作为普通文本块。
- Markdown：保留原始 Markdown，使用安全的围栏封装文档上下文。
- JSON：校验格式并保留原始数字与字符串，避免大整数精度损失。
- CSV / TSV：解析带引号字段、转义引号和单元格内换行，附带行列统计并保留原始数据。
- 常用代码、YAML / TOML / XML 等：按扩展名标记语言，保留源文件。

PDF、Word、Excel 和其他二进制文件暂不解析，上传时明确提示，不会误读为文本。

图片协议参考 [OpenAI Images and vision](https://developers.openai.com/api/docs/guides/images-vision) 与 [Anthropic Vision](https://platform.claude.com/docs/en/build-with-claude/vision)。Windows 的前端拖拽按 [Tauri dragDropEnabled](https://v2.tauri.app/reference/config/#dragdropenabled) 配置。

## 存储目录、同步与配置迁移

在「设置 → 存储与同步」选择目录，或者填写当前设备上的绝对路径。可使用 OneDrive、iCloud Drive、Dropbox、Syncthing 等软件管理的独立文件夹；应用只读写本机文件，不调用这些软件的云端 API。请在同步软件中将整个数据文件夹设置为**始终保留在此设备上**，避免会话文件或图片只有云端占位符。

- 默认数据目录统一为当前用户主目录下的 `.oh-my-chatwise`：macOS 为 `~/.oh-my-chatwise/`，Windows 为 `%USERPROFILE%\.oh-my-chatwise\`。旧默认目录的数据会自动合并迁移到新默认目录，并保留原文件；已选择的自定义目录继续使用。
- 「合并迁移」会将当前变更历史和图片复制到目标目录，与目标目录已有记录合并。迁移成功后才保存本机的目录选择，原目录保留。切换前的会话快照保存在本机应用元数据目录的 `recovery/` 下，便于恢复尚未成功写入旧目录的草稿（图片仍需原目录文件）。
- 取消「合并迁移」会直接打开目标目录，适合另一台设备加入已同步的工作空间，或从不可用的旧目录切换。
- 每台设备分别选择其本机路径。绝对路径、设备标识和当前选中的会话仅写入系统应用数据目录内的 `storage-location.json`（macOS 的 `~/Library/Application Support/com.zexinzhuang.oh-my-chatwise/` 或 Windows 的 `%APPDATA%\com.zexinzhuang.oh-my-chatwise\`），不会进入共享数据或配置导出。
- 图片文件保存在所选目录的 `images/`；文本附件内容、草稿、会话参数和客户端配置一并保存。
- 旧版 `workspace.json` 会自动导入新存储格式，旧文件及原备份保留；迁移后以 `sync-v1/events/` 的记录为准。

### 多客户端冲突

每次保存写入独立、原子创建的 JSON 变更文件，并用已观察到的版本 ID 记录因果关系。**不覆盖一个共享的数据库或工作空间文件，也不依赖设备时钟判断谁更新。**

- 不同会话、不同模型连接、不同提示词及不同客户端设置项的修改自动合并。
- 同一会话、连接、设置或凭据的并发修改保留全部版本，界面提示待处理冲突；未解决前显示一个确定版本。
- 在「存储与同步」展开冲突比较内容，可采用某个版本；会话、模型连接和提示词可以将其他版本保留为独立副本。凭据版本只显示元数据，不在冲突预览中展示密钥。
- 删除保存为同步标记。离线客户端仅再次保存旧内容不会复活记录；离线**修改**与删除相遇时保留修改并提示冲突，可明确选择删除版本。
- 同步软件生成的同内容冲突副本按内部版本 ID 去重。同 ID 不同内容、损坏文件或未来版本会报错并停止写入，保留原数据。
- 应用在空闲且设置关闭时每 10 秒检查本地同步文件，也可点击「立即同步」。生成回复或处理附件时暂缓合并。同步软件实际上传/下载是否完成，请查看该软件状态。
- 历史变更和删除标记不会自动清理，保证长期离线设备仍能合并。当前单次变更上限 32 MB、变更历史读取上限 512 MB；达到上限会停止保存并报错，不会丢弃历史。需要归档时先备份整个目录，再在新目录建立工作空间。

不能保证在同步软件尚未传输某个文件时另一设备立即可见；应用不宣称提供云端实时锁。并发冲突会在相关文件到达本机后识别。请同步整个数据目录，不要单独挑选 JSON 文件，也不要手动删除变更历史或删除标记。

### 本地凭据与同步

应用仅使用本地文件保存凭据，已移除 macOS 钥匙串 / Windows 凭据管理器的实现与 `keyring` 依赖。API Key、完整图床配置和工具配置与会话一起保存在所选数据目录的 `sync-v1/events/*.json` 中；默认目录为 `~/.oh-my-chatwise`。将数据目录设为 OneDrive 等同步软件管理的独立文件夹，在另一客户端选择其本机对应目录并等待下载，即可读取凭据与配置，无需再次授权系统凭据库。

默认凭据为**明文**，能读取同步目录、同步账号或历史备份的程序和人员可以读取密钥。macOS/Linux 新建记录使用仅当前用户读写的文件权限，Windows 继承所在目录权限；本地文件权限不等于加密。请将同步目录设为始终保留在此设备，避免云端占位文件尚未下载。

凭据采用现有按记录、不可变文件的同步机制，不让多客户端反复覆盖同一个 `credentials.json`。不同凭据独立合并；同一凭据的并发修改保留全部版本，在「存储与同步」中选择版本解决冲突。冲突预览隐藏凭据内容，删除标记避免旧值复活。切换目录时勾选合并迁移会同时复制凭据记录；取消迁移则只使用目标目录的数据。

升级时会将此前已迁移的本机 `credentials.json` 内容一次性写入当前数据目录，已有共享记录（含删除标记）优先。原文件保留为恢复副本，不再作为日常凭据来源；本机 `credential-records-migrated.json` 防止重复导入。旧模式选择文件不再使用。尚未从系统凭据库迁出的用户需重新填写密钥或导入配置；此版本不会访问或删除系统凭据库中的任何条目。

可选的凭据加密仍兼容旧加密目录：在「可选凭据加密」中输入原同步密码解锁。启用后，当前及后续凭据使用 `ring` 的 AES-256-GCM、随机 salt/nonce 与 PBKDF2-HMAC-SHA256（600,000 次）保存。密码只保存在本机应用目录的 `credentials.json`，不会随数据目录同步。不同设备需各自输入相同密码。

「改为明文同步」会先验证并解密全部当前凭据，任何失败都会阻止切换；旧加密历史仍保留。启用加密不会擦除已存在于本地事件历史或同步软件历史中的明文。当前不支持原地更换同步密码，更换请使用独立目录。加密配置导入导出的格式保持兼容，会话和图片文件仍为普通本地文件。

### 一键导出与导入配置

「配置导入与导出」使用系统保存对话框导出单个 `chatwise-config.json`。默认包含凭据并要求文件密码，**整个配置包加密**；取消包含凭据时导出普通 JSON。

配置包含：模型连接名称、协议、endpoint、已选择的模型、模型昵称及思考强度、默认连接与模型、提示词库及默认提示词、全局生成参数与工具限额、外观、图片输入模式，以及选择包含凭据时的 API 密钥、完整图床配置（URL、Header、Body、上传方法、模板与响应提取规则）和工具配置（Brave Search 与 API Key、HTTP 接口、MCP 服务与勾选工具及鉴权）。不包含会话、草稿、附件、数据目录、设备标识或凭据同步密码。

另一客户端选择配置文件，输入原文件密码，点击「解析并预览」检查连接数、名称及凭据数量，再导入。默认合并连接、同 ID 更新；也可替换连接与提示词列表。两种模式均保留现有会话和草稿。替换后若某会话引用的连接已移除，需要为该会话重新选择模型。导入不会请求模型 API、获取模型列表、自动测试图床或执行工具。

文件格式带独立的类型与版本标识，不接受工作空间/会话 JSON 作为配置包；未知版本、无效 endpoint、错误密码或损坏密文在写入前拒绝。配置文件上限 4 MB。

### 请求与显示

模型请求由 Rust `reqwest` 直接发送到用户填写的地址，不经过应用方服务器。启用图床 URL 模式时，选择的本地图片还会上传到用户配置的图床或 OSS。禁用 HTTP 重定向，避免认证信息被带往其他端点。Markdown 禁止原始 HTML，并通过 DOMPurify 清理；应用 CSP 不允许远程脚本。用户主动添加的公网图片可在附件组件预览，模型回复的 Markdown 不加载远程图片。

原生目录选择和配置保存使用 [Tauri 官方 Dialog 插件](https://v2.tauri.app/plugin/dialog/)。浏览器预览不提供桌面文件存储或配置迁移功能。

## 验证与打包

```sh
pnpm test
pnpm build
pnpm format:check
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml
```

后端纯单元测试覆盖循环限额、授权拒绝、取消与超时、三种协议的工具续写、加密工具配置迁移和模板鉴权清理；另有本地模拟 HTTP 服务测试验证三种协议的认证头、请求体、SSE 中文/emoji 分片、结束事件和错误响应，不需要真实 API 密钥，不产生模型费用。前端测试覆盖会话隔离、工具记录回填、逐次授权、停止后拒绝授权、请求前保存失败、历史上下文、重试、损坏数据保护与中断恢复。

在对应操作系统上打包：

```sh
pnpm tauri build
```

产物在 `src-tauri/target/release/bundle/`；macOS 生成 `.app` / `.dmg`，Windows 生成 NSIS `.exe` / MSI `.msi`。跨平台打包请使用对应系统的构建环境。调试包可通过 `pnpm tauri build --debug --bundles app` 在 macOS 构建。

### GitHub Actions 自动构建与发版

仓库：[zzxCNCZ/oh-my-chatwise](https://github.com/zzxCNCZ/oh-my-chatwise)。流程在 [`.github/workflows/desktop.yml`](.github/workflows/desktop.yml)，目前只构建 **macOS Apple Silicon ARM64**（`aarch64-apple-darwin`），使用 `macos-15` 云端 runner，不依赖本机编译。

| 触发方式 | 结果 |
| --- | --- |
| 推送 `main` / `master` | 检查、测试、构建，在 Actions 的 Artifacts 下载产物 |
| 向主分支提交 PR | 同样验证构建，不创建 Release |
| Actions → macOS ARM64 build and release → Run workflow，选择分支 | 手动构建，不创建 Release |
| 推送 `v0.1.0` 等版本标签 | 检查、测试、构建成功后自动发布 GitHub Release |
| 推送 `v0.2.0-beta.1` 等预发布标签 | 自动发布 Pre-release，不更新 Latest |

每次产出 `.dmg` 安装包、`.app.zip` 应用压缩包和 `SHA256SUMS.txt`。分支构建产物保留 14 天；Release 附件长期保留。流程验证实际二进制为 `arm64`，并在发布前重新核对下载的文件校验和。所有检查和构建完成后才创建 Release 草稿，附件上传成功后才公开发布；失败时不会公开一个缺失附件的新 Release。重跑可以继续上传未完成的草稿，已公开版本不覆盖，请使用新版本标签。

首次发布当前 `0.1.0` 版本：

```sh
node scripts/check-release-version.mjs v0.1.0
git tag -a v0.1.0 -m "Release v0.1.0"
git push origin v0.1.0
```

后续发版前统一修改 `package.json`、`src-tauri/tauri.conf.json`、`src-tauri/Cargo.toml` 和 `src-tauri/Cargo.lock` 中 **oh-my-chatwise 包条目**的版本，提交并推送后再打相同版本的 `v` 标签。不要修改 Cargo.lock 中其他依赖的版本。流程会在安装依赖前检查这四处版本和标签，不一致则立即停止。

无需手动配置 GitHub PAT：发布 job 使用仓库自带的 `GITHUB_TOKEN`，仅该 job 申请 `contents: write`；构建和 PR job 保持只读。依赖安装使用 pnpm / Cargo 锁文件，Rust 编译缓存用于后续构建。

当前包使用 **ad-hoc 签名**，不需要上传证书，但不等同于 Apple Developer ID 签名及公证。macOS 首次打开下载的应用可能提示无法验证开发者，需要在「系统设置 → 隐私与安全性」中允许打开。面向公众正式分发时，需要另行配置自己的 Apple Developer ID 证书与公证凭据；此流程不将应用标识或本地用户配置改为其他值，也未启用应用内自动更新。

参考：[Tauri GitHub 构建指南](https://v2.tauri.app/distribute/pipelines/github/)、[GitHub runner 说明](https://docs.github.com/en/actions/reference/runners/github-hosted-runners)、[Tauri macOS 签名与公证](https://v2.tauri.app/distribute/sign/macos/)。

## 目录

```text
src/
  App.vue                       界面与交互编排
  components/                   设置弹窗、Markdown、应用图标
  lib/
    types.ts                    前后端数据约定
    bridge.ts                   Tauri 调用与浏览器预览边界
    useModelCatalog.ts          模型发现、搜索选择与过期响应隔离
    useWorkspace.ts             会话状态、生成流程、串行持久化
    workspace.ts                初始化、数据恢复、导出
    markdown.ts                 Markdown 渲染与清理
src-tauri/src/
  lib.rs                        Tauri 命令、请求生命周期与取消
  catalog.rs                    模型列表获取、认证与分页
  attachments.rs                本地图片存储、类型校验与上下文解析
  image_upload.rs               图床 / OSS 上传与 URL 提取
  agent/                        工具循环、协议续写、HTTP / MCP、授权与限额
  llm.rs                        三种协议的文本与图片请求适配
  sse.rs                        增量 SSE 字节解析
  storage.rs                    本地持久化与备份
  credentials.rs                本机升级恢复副本、迁移标记与可选同步密码
  models.rs                     Rust 数据类型
```
