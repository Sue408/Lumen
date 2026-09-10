# 本地部署 LLM 聚合网关：API 返回格式与用量字段归一化调研

## 摘要：以 OpenAI Chat 作为外接口，以“六类规范量 + 原始证据”作为内部模型

**调研截止日：2026-09-10。**

本报告面向自用 LLM 聚合网关，结论是：**外接口固定为 OpenAI Chat Completions，内部领域模型保存“六类规范量 + 提供商原始用量”**，而不是试图让几十家 API 在 JSON 字段上完全一致。OpenAI Chat 拥有完整的请求、流式增量、工具调用、音频和用量细节语义，是当前兼容性成本最低的外接口基线。

不同 API 的 usage 并非简单字段名差异，而是计量边界不同：OpenAI Responses 的 `input_tokens` 包含缓存命中的输入，DeepSeek 的 `prompt_cache_hit_tokens` 是从输入中拆出的命中部分，Anthropic 的 `input_tokens` 包含缓存读取且另外报告 `cache_read_input_tokens` 与 `cache_creation_input_tokens`，Gemini 的 `promptTokenCount` 在存在缓存时也包含缓存内容。[1][2][3][4]

因此，网关如果只把不同字段机械相加，可能重复计费或漏计缓存写入。稳健设计是分别保留“输入、输出、缓存读取、缓存创建、推理、音频”六类规范量，以及各自是否包含、是否独立的语义标记。

第二大风险是**流式用量不可靠**。OpenAI Chat 只有在显式设置 `stream_options.include_usage=true` 时，才在 `[DONE]` 前发送一个 `choices=[]`、但包含完整请求用量的末块；其他 chunk 的 `usage` 为 `null`，流中断则可能丢失末块。Anthropic 则分别通过 `message_start.usage.input_tokens` 与后续 `message_delta.usage.output_tokens` 传递字段，并非所有信息都在最后一个事件中。[1][5]

中转网关还会带来第二层失真：模型映射可能使 `model` 不等于真实后端模型；用户余额、组配额和模型价格通常位于 `/api/status`、`/api/quota`、`/api/dashboard` 等非标准端点；私有实现还可能把未使用的字段置为 `0`、`-1`、`null`，甚至省略。[6][7] 本地推理后端的问题则是能力参差不齐：Ollama 原生 API 返回的是 `prompt_eval_count`/`eval_count` 与耗时，而非 OpenAI usage；LM Studio 已经补齐标准 usage 和 `stats`；Xinference 公开示例出现过 `usage=-1`。[8][9][10]

**工程上应优先保证三件事：**

1. 从真实生产响应中提取字段，宁可保留原始 JSON，也不根据文档替供应商补齐数值；
2. 将计费、限额、模型路由依据和审计证据分开存储；
3. 将流式用量从内容转发链路中剥离，统一在终端事件、流结束或流中断时结算。

未找到公开官方字段证据的厂商，本报告明确标注为“未确认”，不为其编造字段名。

## 1. 外部协议固定为 OpenAI Chat，内部按“量、边界、来源”三元组建模

**协议归一化的目标不是统一字段拼写，而是统一计量含义。** 网关必须区分“可计费量”“包含关系”和“原始证据”；只复制字段名而忽略边界，最终仍会在缓存、推理或工具调用场景算错。

OpenAI Chat Completions 应成为唯一稳定外接口，因为它同时覆盖同步与流式响应、`finish_reason`、普通文本与工具调用、函数调用、`logprobs`、`system_fingerprint`、`service_tier` 及详细 usage。内部则不要直接绑定 OpenAI 的扁平结构，而应采用规范领域模型。

```text
ProviderRawResponse
 -> ProviderAdapter.parse()
 -> NormalizedCompletion {
      id, provider, model(逻辑), model_real(后端),
      messages, tool_calls, finish,
      usage: CanonicalUsage,
      raw_usage: object  // 审计、调试、未来映射
    }
```

### 1.1 规范用量必须同时记录数值、包含关系和来源

建议 `CanonicalUsage` 至少包含以下字段：

| 字段 | 含义 | 单位/类型 |
|---|---|---|
| `input_tokens` | 输入计费计量；明确是否包含缓存命中部分 | int64 |
| `output_tokens` | 模型生成输出 | int64 |
| `cache_read_tokens` | 本次从缓存读取的输入 token | int64 |
| `cache_creation_tokens` | 本次写入缓存的输入 token | int64 |
| `reasoning_tokens` | 推理/思考 token；注意是否属于 output | int64 |
| `input_audio_tokens` / `output_audio_tokens` | 音频 token | int64 |
| `total_tokens` | 网关定义的统一总量 | int64 |
| `contains_cache_read` | `cache_read_tokens` 是否已计入 `input_tokens` | bool |
| `contains_reasoning` | `reasoning_tokens` 是否已计入 `output_tokens` | bool |
| `source` | `provider` / `gateway_estimated` / `missing` | enum |

如果不保存 `contains_*` 标志，后续计费逻辑无法判断“命中 token 应单独收费，还是已经从输入总量中扣除”。这比字段名本身更重要。

### 1.2 各家总量没有统一加法公式

`total_tokens` 不能写成一个适用于所有提供商的公式。例如：

- OpenAI Chat 的 `total_tokens = prompt + completion`；
- Gemini 的 `totalTokenCount` 可能包含 thoughts；
- DeepSeek、Anthropic 的缓存命中部分已经包含在输入中，不应再叠加。

因此，网关应优先信任各提供商返回的 `total`，只在缺失时使用与**该提供商计量边界一致**的公式计算，并把结果标记为 `estimated`。

## 2. 六类规范量是跨厂商映射锚点，细节字段应只用于展示与调价

**跨厂商归一化的稳定锚点是六类规范量，而不是 JSON 路径。** 完整细节字段应放在 `provider_specific` 中，避免为兼容少量高级字段而污染核心模型。

| 语义 | OpenAI Chat | OpenAI Responses | Anthropic Messages | DeepSeek Chat | Google Gemini | OpenRouter Chat 兼容 |
|---|---|---|---|---|---|---|
| 输入 token | `usage.prompt_tokens` | `usage.input_tokens` | `usage.input_tokens` | `usage.prompt_tokens` | `usageMetadata.promptTokenCount` | `usage.prompt_tokens` |
| 输出 token | `usage.completion_tokens` | `usage.output_tokens` | `usage.output_tokens` | `usage.completion_tokens` | `usageMetadata.candidatesTokenCount` | `usage.completion_tokens` |
| 缓存读取 | `prompt_tokens_details.cached_tokens` | `input_tokens_details.cached_tokens` | `cache_read_input_tokens` | `prompt_cache_hit_tokens` | `cachedContentTokenCount` | `prompt_tokens_details.cached_tokens` |
| 缓存创建/写入 | Responses/Chat 规范中未作为通用字段确认；按具体模型与文档判断 | 未在本次官方证据中确认 | `cache_creation_input_tokens` 及 `cache_creation.{ephemeral_5m,ephemeral_1h}` | DeepSeek 文档只确认命中/未命中，未确认独立写入量 | Gemini 未确认独立写入量 | `prompt_tokens_details.cache_write_tokens`（可选） |
| 推理/思考 | `completion_tokens_details.reasoning_tokens` | `output_tokens_details.reasoning_tokens` | 不单独在 usage 中返回 | `completion_tokens_details.reasoning_tokens` | `thoughtsTokenCount` | `completion_tokens_details.reasoning_tokens` |
| 音频输入 | `prompt_tokens_details.audio_tokens` | 未在本次摘录中确认 | 原生接口未确认；取决于模型/工具 | 未确认 | 通过 `promptTokensDetails[].modality` 扩展 | `prompt_tokens_details.audio_tokens` |
| 音频输出 | `completion_tokens_details.audio_tokens` | 未在本次摘录中确认 | 原生接口未确认 | 未确认 | 通过 `candidatesTokensDetails[].modality` 扩展 | `completion_tokens_details.audio_tokens` |
| 工具使用输入 | 未作为通用 Chat 字段确认 | 未在本次摘录中确认 | 通常计入输入；`server_tool_use` 另计 | 未确认 | `toolUsePromptTokenCount` | `server_tool_use_details`（非 token） |
| 总 token | `total_tokens` | `total_tokens` | 通常按 `input+output` 推算，需按具体响应确认 | `total_tokens` | `totalTokenCount` | `total_tokens` |

### 2.1 细节字段应按功能分级

**L1 是计费与限额必需字段：**输入、输出、缓存读取、缓存创建、推理、总 token。

**L2 是产品功能字段：**音频、视频、`accepted_prediction`、`rejected_prediction`、工具调用、模态明细。

**L3 是展示字段：**指纹、service tier、provider、区域、路由、模型版本。

这种分层可以防止两个问题：一是把“响应里没有”误判成“值为 0”；二是避免把展示字段混入计费主链路。

### 2.2 `cached_tokens` 不能无差别等同于“节省了这些输入 token”

OpenAI Chat 的 `prompt_tokens_details.cached_tokens`、OpenRouter 的同名字段和 DeepSeek 的 `prompt_cache_hit_tokens` 都表示命中输入，但计费处理并不完全一致。

Anthropic 的 `cache_read_input_tokens` 和 `cache_creation_input_tokens` 额外区分读取与创建。网关应按以下规则转换：

```text
read = cache_read
created = cache_creation
# DeepSeek：命中部分已是 prompt 的子集
input_excl_cache = prompt - hit
# Anthropic：input 包含 read；创建量一般另行计费
input_excl_cache = input - cache_read
```

如果缺少写入量，就不应推导写入成本；如果缺少命中量，也不应假设缓存已生效。

## 3. OpenAI Chat 是归一化基线，但 Responses 改变了顶层语义

**OpenAI Chat Completions 适合作为统一外接口，但 Responses API 不能被视为同一协议的简单包装。** 两者不仅字段名不同，响应组织方式、状态模型和缓存口径也不同。

### 3.1 Chat Completions 的完整同步响应结构

```json
{
  "id": "chatcmpl-123",
  "object": "chat.completion",
  "created": 1719300000,
  "model": "gpt-5",
  "system_fingerprint": "fp_xxx",
  "service_tier": "default",
  "choices": [
    {
      "index": 0,
      "message": {
        "role": "assistant",
        "content": "Hello!",
        "tool_calls": [
          {
            "id": "call_1",
            "type": "function",
            "function": {
              "name": "get_weather",
              "arguments": "{\"city\":\"Shenzhen\"}"
            }
          }
        ],
        "function_call": null
      },
      "finish_reason": "tool_calls",
      "logprobs": null
    }
  ],
  "usage": {
    "prompt_tokens": 100,
    "completion_tokens": 20,
    "total_tokens": 120,
    "prompt_tokens_details": {
      "cached_tokens": 64,
      "audio_tokens": 0
    },
    "completion_tokens_details": {
      "reasoning_tokens": 0,
      "audio_tokens": 0,
      "accepted_prediction_tokens": 0,
      "rejected_prediction_tokens": 0
    }
  }
}
```

OpenAI 官方文档确认：`finish_reason` 可为 `stop`、`length`、`tool_calls` 或已弃用的 `function_call`；`content_filter` 表示内容被过滤器移除。`service_tier` 标识请求的处理服务层级，`system_fingerprint` 描述运行模型的后端配置。[1]

### 3.2 标准 Chat 流必须显式开启 `include_usage`

```json
{
  "stream": true,
  "stream_options": {
    "include_usage": true,
    "include_obfuscation": false
  }
}
```

此时，最终 chunk 的结构为：

```json
{
  "id": "chatcmpl-123",
  "object": "chat.completion.chunk",
  "choices": [],
  "usage": {
    "prompt_tokens": 100,
    "completion_tokens": 20,
    "total_tokens": 120,
    "prompt_tokens_details": {"cached_tokens": 64, "audio_tokens": 0},
    "completion_tokens_details": {
      "reasoning_tokens": 0,
      "audio_tokens": 0,
      "accepted_prediction_tokens": 0,
      "rejected_prediction_tokens": 0
    }
  }
}
```

OpenAI 的规则明确：

- `include_usage` 只在 `stream=true` 时有效；
- 启用后，最终 chunk 位于 `data: [DONE]` 之前；
- 该 chunk 的 `choices` 为空；
- 只有该 chunk 包含整个请求的 token 统计；
- 其余 chunk 的 `usage` 为 `null`；
- 如果流中断，最终 usage chunk 可能无法送达，但不代表请求没有消耗 token。[1]

因此，网关不能只依赖最后一个内容 chunk 的 `finish_reason` 判断用量是否完整。应独立跟踪：

```text
stream_state = {
  content_done: bool,
  final_usage_seen: bool,
  accumulated_usage: CanonicalUsage | null
}
```

如果流正常完成，但 `final_usage_seen=false`，应将 usage 标记为 `partial`，并在流结束后从累计内容估算输出；只有在业务允许时才补写。对于计费链路，应优先使用后台审计任务重新核对，而不是直接拿估算值收费。

### 3.3 Responses API 不能简单降级为 Chat 兼容响应

Responses API 是非流式示例中的典型结构：

```json
{
  "id": "resp_123",
  "object": "response",
  "created_at": 1719300000,
  "status": "completed",
  "model": "gpt-5",
  "output": [
    {"type": "reasoning", "id": "rs_1", "summary": [{"type":"summary_text","text":"..."}]},
    {"type": "message", "id": "msg_1", "role": "assistant",
     "content": [{"type": "output_text", "text": "Hello!"}]}
  ],
  "incomplete_details": null,
  "usage": {
    "input_tokens": 75,
    "input_tokens_details": {"cached_tokens": 0},
    "output_tokens": 1186,
    "output_tokens_details": {"reasoning_tokens": 1024},
    "total_tokens": 1261
  }
}
```

Chat 与 Responses 的主要差异如下：

| 维度 | Chat Completions | Responses |
|---|---|---|
| 顶层容器 | `choices[]` | `output[]` |
| 内容角色 | `choices[].message` | `output[]` 中的 `message`、`reasoning` 等 |
| 输入结构 | `messages` | `input` |
| 停止信息 | `finish_reason` | `status`、`incomplete_details` |
| 工具调用 | `message.tool_calls` | `output[]` 中的 `tool_call` 等 |
| 聚合文本 | `choices[].message.content` | 使用 `output_text` 便利属性拼接 |

官方推理模型文档明确：`output_tokens` 包括可见输出、不可见推理和其他输出；`incomplete_details.reason` 可能为 `max_output_tokens`。因此，即便文本为空，也可能已经发生输入和推理费用。[2]

### 3.4 Responses 的流式映射必须转换为事件流

Responses 原生使用 JSON 事件流，而 Chat 外接口使用 SSE。网关至少应转换以下事件：

`response.created/updated/completed`  
→ 映射为响应元数据，不直接生成 choice delta。

`response.output_item.added/updated`  
→ 创建或更新 `output[]`。

`response.output_text.delta`  
→ 映射为 `choices[0].delta.content`。

`response.reasoning_text.delta`  
→ 映射为扩展字段或 `reasoning_content` delta；不得混入标准 `content`。

`response.tool_call.*`  
→ 映射为 `tool_calls` delta。

`response.completed`  
→ 从最终 `response.usage` 生成 Chat 末块。

如果在 `response.incomplete` 时终止流，应保留 `incomplete_details`，不能把 `finish_reason` 一律设为 `stop`。本次调研未找到官方文档承诺 Responses 的每个事件都携带完整 usage，因此应以最终响应对象的 `usage` 为权威计费源。

## 4. Anthropic 把输入、输出和缓存拆开报告，流式中必须累积与覆盖并用

**Anthropic 的关键不是字段多，而是同一用量分布在多个事件和多个对象中。** 网关必须识别“首次出现、持续累积、最终覆盖”三种更新语义，不能对相同字段做无条件的整条覆盖。

### 4.1 非流式响应包含内容块与完整 usage

```json
{
  "id": "msg_01",
  "type": "message",
  "role": "assistant",
  "model": "claude-opus-5",
  "container": null,
  "content": [
    {"type": "thinking", "thinking": "...", "signature": "..."},
    {"type": "redacted_thinking", "data": "..."},
    {"type": "text", "text": "Hello!"},
    {"type": "tool_use", "id": "toolu_1", "name": "get_weather", "input": {"city":"SZ"}},
    {"type": "server_tool_use", "id":"srvtoolu_1","name":"web_search","input":{}}
  ],
  "stop_reason": "tool_use",
  "stop_sequence": null,
  "usage": {
    "input_tokens": 500,
    "output_tokens": 120,
    "cache_creation_input_tokens": 400,
    "cache_read_input_tokens": 100,
    "cache_creation": {
      "ephemeral_5m_input_tokens": 300,
      "ephemeral_1h_input_tokens": 100
    },
    "server_tool_use": {"web_search": {"requests": 1}},
    "service_tier": "standard"
  }
}
```

官方停止原因包括：`end_turn`、`max_tokens`、`stop_sequence`、`tool_use`、`pause_turn`、`refusal`、`model_context_over_window_exceeded`。[3]

### 4.2 Anthropic 的缓存写入与读取必须分别计量

`cache_creation.ephemeral_5m_input_tokens` 与 `ephemeral_1h_input_tokens` 表示按 TTL 分组的缓存创建量；请求级总量仍同时出现在 `cache_creation_input_tokens`。`cache_control` 的 `ttl` 可为 `5m` 或 `1h`。[3]

这一结构意味着：缓存写入不是命中量的镜像。同一请求可以同时存在读取和写入，也可能因缓存系统行为只返回部分信息。网关应按照以下四条规则处理：

1. `cache_read_input_tokens` 已从 `input_tokens` 中扣除，不能再加回输入总量；
2. `cache_creation_input_tokens` 是当前请求新写入的缓存量，应按写入价单独计费；
3. 不得由 `read` 反推 `creation`，也不得由 `prompt_tokens - read` 反推写入；
4. 如果只有总量，没有 `cache_creation` 分组，就不能断言具体 TTL。

### 4.3 官方兼容层仍需逐项验证

Anthropic 原生 SDK 使用 `Message` 结构，第三方 OpenAI 兼容层则可能从 Messages 反向转换为 Chat。其高风险点包括：

- `thinking` / `redacted_thinking` 可能丢失，或被错误放入 `content`；
- `tool_use` 可能转换为 `tool_calls`，但 `input` JSON 的流式拼接需要严格校验；
- `stop_reason` 可能映射为 OpenAI 的 `finish_reason`；
- `container`、`server_tool_use`、扩展 `usage` 通常在 Chat 响应中不存在；
- `cache_creation.*` 分 TTL 字段经常缺失。

因此，网关不应依赖“声称兼容 Anthropic”的宣传。应在 CI 中使用真实响应快照，重点覆盖工具调用、扩展思考、缓存和流式场景。

### 4.4 流式事件必须按字段语义合并

官方事件链为：

1. `message_start`：携带初始 `Message`，`content` 为空，并提供 `usage.input_tokens` 等输入信息；
2. 每个内容块依次发送 `content_block_start`、`content_block_delta`、`content_block_stop`；
3. 可发送一个或多个 `message_delta`，更新顶层字段；
4. 最后发送 `message_stop`；
5. 任意位置可能出现 `ping`。

官方示例显示，`message_start` 已包含输入与部分输出计数，`message_delta` 再更新 `output_tokens`：

```text
message_start:
  usage = { input_tokens: 25, output_tokens: 1 }

message_delta:
  usage = { output_tokens: 15 }
```

因此，归一化状态机应采用以下合并规则：

```text
on message_start:
  state.input = event.usage.input_tokens
  state.output = event.usage.output_tokens（若存在）
  state.read  = event.usage.cache_read_input_tokens
  state.created= event.usage.cache_creation_input_tokens

on message_delta:
  if usage.output_tokens exists:
    state.output = usage.output_tokens   // 覆盖为累计值
  state.read  = usage.cache_read_input_tokens  ?? state.read
  state.created= usage.cache_creation_input_tokens ?? state.created

on message_stop:
  finalize()
```

字段出现位置可以总结为：

| 字段 | 常见位置 | 处理方式 |
|---|---|---|
| `input_tokens` | `message_start.usage` | 首次赋值 |
| `output_tokens` | `message_start` 初值及 `message_delta.usage` | 覆盖为最新累计值 |
| `cache_read_input_tokens` | 早期事件或后续 delta | 保留最新非空值 |
| `cache_creation_input_tokens` | 早期事件或后续 delta | 保留最新非空值 |
| `stop_reason` | `message_delta.delta` | 更新并用于最终映射 |

`input_tokens` 通常在开始阶段已知，但缓存细节未必一定在 `message_start` 中。因此，应保留“事件级字段覆盖”，不能只做一次性拷贝。

## 5. 国内厂商应以“真实快照 + 网关补丁”接入，不能依赖文档承诺

**国内 OpenAI 兼容端点最容易同时出现三个问题：字段缺失、模型映射失真、缓存语义不完整。** 因此，接入方式应是先录制真实响应，再把差异写入可版本化的适配器补丁。

### 5.1 DeepSeek 同时提供 OpenAI 与 Anthropic 接口，但能力并不对称

DeepSeek Chat 兼容接口的典型 usage：

```json
{
  "usage": {
    "prompt_tokens": 16,
    "completion_tokens": 10,
    "total_tokens": 26,
    "prompt_cache_hit_tokens": 8,
    "prompt_cache_miss_tokens": 6,
    "completion_tokens_details": {"reasoning_tokens": 4}
  }
}
```

官方文档确认，`prompt_cache_hit_tokens` 与 `prompt_cache_miss_tokens` 共同反映请求输入中的缓存命中与未命中情况；`reasoning_content` 用于思考模式，流式时也可作为增量字段出现。[11][12]

映射规则应为：

```text
input      = prompt_tokens
output     = completion_tokens
cache_read = prompt_cache_hit_tokens
# DeepSeek 文档未确认独立 cache_creation 量
cache_creation = raw.prompt_cache_miss_tokens 仅作“候选写入量”，不可直接计费
reasoning = completion_tokens_details.reasoning_tokens
contains_cache_read = true
```

**`prompt_cache_miss_tokens` 不等于 `cache_creation_input_tokens`。** 前者是输入中未命中的部分，后者是新写入缓存的量；在没有官方计费口径前，不能用未命中量推导写入费用。

DeepSeek 的官方 Anthropic 兼容层也不是能力超集。公开兼容性清单显示：

- `cache_control` 被忽略；
- `container`、`mcp_servers` 被忽略；
- `service_tier` 被忽略；
- `metadata` 中仅 `user_id` 得到支持，其余字段被忽略；
- `tool_choice` 的部分能力受支持；
- `redacted_thinking` 不受支持。[12]

这意味着，`thinking` 可以存在，但 `cache_control` 不生效，缓存命中也不应被期望在 Anthropic 风格响应中完整回读。从 OpenAI 格式转换到 DeepSeek Anthropic 接口时，网关必须删除不支持的缓存标记，并记录降级事件。

截至调研日，本次查阅的官方 DeepSeek Chat schema 未显示 `prompt_tokens_details.cached_tokens`。因此：

- DeepSeek OpenAI 兼容接口优先使用 `prompt_cache_hit_tokens`；
- 不应假设存在 OpenAI 风格的嵌套 `cached_tokens`；
- 如果实际版本出现该字段，应以真实响应为准。

### 5.2 百炼 DashScope 已公开 OpenAI 兼容缓存字段

火山方舟公开示例包含：

```json
"usage": {
  "prompt_tokens": 28,
  "completion_tokens": 4,
  "total_tokens": 32,
  "prompt_tokens_details": {"cached_tokens": 18},
  "completion_tokens_details": {"reasoning_tokens": 5}
}
```

这表明其 OpenAI 兼容层已使用与 OpenAI 相近的嵌套字段。[13] 映射时可直接读取：

```text
cache_read = prompt_tokens_details.cached_tokens
reasoning  = completion_tokens_details.reasoning_tokens
```

但公开页面未确认独立的缓存创建量、TTL 或 `prompt_tokens` 的精确包含边界，因此这些字段应保留为 `unknown`，不能自行补齐。

### 5.3 其他国内厂商应优先记录已知事实，其余标记为未确认

| 提供商 | 已确认或较高可信度事实 | 缓存口径与缺失风险 |
|---|---|---|
| Moonshot Kimi | OpenAI 兼容 Chat；公开站点确认兼容 SDK 与调用方式 | 本次未找到官方稳定 usage schema；不承诺 `cached_tokens` |
| 智谱 GLM | 官方文档声明 OpenAI SDK 兼容 | 本次未发现可引用、稳定公开的 usage 细节；不写具体字段 |
| 阿里百炼 DashScope | OpenAI 兼容；公开价格页区分普通输入、缓存命中、Batch 和缓存创建价格 | 不同模型和端点可能不同；应从真实响应提取，不把网页价格写成响应字段 |
| 火山方舟/豆包 | 已公开 `prompt_tokens_details.cached_tokens` 和 `reasoning_tokens` | 创建量、TTL 未确认；命中部分与输入包含关系按 OpenAI 语义处理 |
| 腾讯混元 | 同时存在原生和兼容入口；原生 API 3.0 数据结构与 OpenAI Chat 并不相同 | 本次未找到可引用 usage 证据；网关应录制真实快照 |
| 百度千帆 | `/v2/chat/completions` OpenAI 兼容，使用 Bearer 鉴权 | 具体 `usage.*` 细节未在本次摘录中确认 |
| MiniMax | 通常对外宣传 OpenAI 兼容 | 本次未找到权威、稳定 usage 证据；缓存与推理字段标 `unverified` |
| 讯飞星火 | 存在 OpenAI 兼容能力 | 本次未确认官方 usage 细节；原生协议差异较大 |
| 360智脑 | 公开 API 资料与兼容层版本变化较快 | 本次未找到可引用规范；不要假设字段存在 |
| 阶跃星辰 | 存在 OpenAI 兼容端点 | 本次未确认官方 usage 细节 |

**接入模板应为：**

```text
1. 录制 3 类真实响应：普通生成、工具调用、缓存/长上下文（如支持）。
2. 提取 raw_usage 快照与 HTTP 头、model、X-Request-Id。
3. 编写 provider adapter 补丁，只映射实际出现的字段。
4. 在快照测试中禁止“字段不存在 => 0”的隐式转换。
```

这种策略比预先建立一张“全国产字段表”更可靠。因为兼容层可能在小版本中新增 `prompt_tokens_details`，也可能根据模型返回不同结构；真实快照才是唯一可信输入。

### 5.4 聚合平台价格与用量必须分开返回

OpenRouter 是较罕见的直接返回费用的案例：

```json
{
  "usage": {
    "prompt_tokens": 10,
    "completion_tokens": 15,
    "total_tokens": 25,
    "prompt_tokens_details": {"cached_tokens": 2},
    "completion_tokens_details": {"reasoning_tokens": 5},
    "cost": 0.0012,
    "cost_details": {
      "upstream_inference_cost": null,
      "upstream_inference_prompt_cost": 0.0008,
      "upstream_inference_completions_cost": 0.0004
    },
    "is_byok": false,
    "server_tool_use_details": {"tool_calls_executed":2,"tool_calls_requested":2}
  },
  "provider": {"name":"OpenAI","specific_model":"gpt-5"}
}
```

官方 TypeScript 定义进一步显示，`cost` 以 credits 计价；`prompt_tokens_details` 还可包含 `cache_write_tokens`、`audio_tokens`、`video_tokens`，`completion_tokens_details` 还可包含 `image_tokens`。流式情况下，usage 会在 `[DONE]` 前的最后一个 chunk 中返回，且 `choices` 为空。[14][15]

因此，OpenRouter 可映射为：

```text
canonical.cost.usd_equivalent = usage.cost
canonical.provider_model = provider.specific_model
```

但其货币单位仍是“credits/美元类金额”，不能直接等同于账户余额。应将 `cost` 作为标记 `provider_reported` 的参考值，最终内部结算仍以网关价格表为准。

对于 Groq、Together、Fireworks、Mistral、Cohere、xAI、Perplexity、Replicate，本报告**不编造统一字段**。这些平台通常分为三类：

- OpenAI 兼容层：可映射标准 Chat usage；
- 原生 API：响应结构差异较大；
- 聚合层：可能附加 `provider`、`cost`、`latency`。

例如，Groq 的 `queue_time`/`completion_time` 即使存在，也属于性能字段，不应进入 token usage。接入前应分别录制非流式与流式响应，再决定是否启用扩展字段。

## 6. Gemini 原生与 OpenAI 兼容层必须双轨解析

**Gemini 的缓存和思考量属于原生 `usageMetadata`，OpenAI 兼容层未必完整暴露。** 网关应先识别响应协议，再决定使用 `promptTokenCount` 还是 `prompt_tokens`。

### 6.1 原生 Gemini 的结构化程度最高

```json
{
  "candidates": [
    {"content":{"parts":[...],"role":"model"},"finishReason":"STOP"}
  ],
  "usageMetadata": {
    "promptTokenCount": 100,
    "cachedContentTokenCount": 60,
    "candidatesTokenCount": 20,
    "thoughtsTokenCount": 8,
    "toolUsePromptTokenCount": 5,
    "totalTokenCount": 133,
    "promptTokensDetails":[{"modality":"TEXT","tokenCount":90},{"modality":"IMAGE", "tokenCount":10}],
    "cacheTokensDetails":[{"modality":"TEXT","tokenCount":60}],
    "candidatesTokensDetails":[],
    "toolUsePromptTokensDetails":[],
    "serviceTier":"standard"
  }
}
```

Google 官方说明特别明确：**当设置 `cachedContent` 时，`promptTokenCount` 仍是完整有效 prompt 大小，包含缓存内容。** `totalTokenCount` 则为 `prompt + thoughts + response candidates`。[4]

因此，Gemini 的正确映射是：

```text
input          = promptTokenCount
output         = candidatesTokenCount
cache_read     = cachedContentTokenCount
reasoning      = thoughtsTokenCount
tool_input     = toolUsePromptTokenCount
contains_cache_read = true
contains_reasoning  = true   // 是否纳入 output 取决于具体 Gemini 版本
total = raw.totalTokenCount if present else prompt + candidates
```

`cachedContentTokenCount` 不能再加回 `promptTokenCount`，`thoughtsTokenCount` 也不应无条件加到 `candidatesTokenCount`。

### 6.2 兼容层映射以真实响应为准

Google 的 OpenAI 兼容层可能直接返回：

```json
"usage": {
  "prompt_tokens": 100,
  "completion_tokens": 20,
  "total_tokens": 133,
  "prompt_tokens_details": {"cached_tokens": 60},
  "completion_tokens_details": {"reasoning_tokens": 8}
}
```

但本次未找到可引用的官方页面，明确承诺所有 Gemini 模型与版本都稳定提供这一结构。因此，网关应：

- 先判断响应是否为 Gemini 原生协议；
- 原生协议使用 `usageMetadata`；
- OpenAI 兼容响应使用 Chat usage；
- 两者通过 provider、base URL 和响应形状路由，避免由字段存在性猜测。

## 7. 中转网关的主要风险来自改写和私有端点，而非格式不兼容

**New API、One API、VoAPI 及镜像站的本质是反向代理加账务系统。** 它们通常承诺对外输出 OpenAI Chat，但模型映射、错误重写、用量透传和价格接口都会影响真实结果。

### 7.1 逻辑模型、真实模型与计费模型可能三者不同

请求中的 `model` 可以是：

- 渠道映射键；
- 租户可见的逻辑模型；
- 用户配额模型；
- 实际后端模型 ID。

后端返回后，网关可能再次改写为逻辑模型。如果不保存 `model_real`，日志中的 usage 可能无法匹配上游价格。

建议每个响应统一写入以下四个字段：

```text
model            # 面向客户
model_logical    # 路由键
model_real       # 上游请求 Host + 实际 model
model_version    # 实际修订版或 tag，如可获得
```

计费只使用 `model_real`；面向客户的统一表示使用 `model`。

### 7.2 New API 文档确认支持跨协议与缓存账务，但具体实现版本敏感

New API 仓库描述包括：

- OpenAI Compatible ⇄ Claude Messages；
- OpenAI Compatible ↔ Google Gemini；
- Claude Messages → OpenAI Compatible；
- Google Gemini → OpenAI Compatible；
- 组织级按请求、用量和缓存命中的成本账务；
- OpenAI、Azure、DeepSeek、Claude、Qwen 及相关模型的缓存计费统计。[16]

这意味着它已经具备转换和账务能力，但不代表任意版本都能完整透传所有扩展字段。私有 fork 可能改写：

- `cached_tokens`；
- `reasoning_tokens`；
- `tool_calls`；
- `usage` 末块；
- `model`；
- `id`；
- 错误结构。

生产环境应保存上游原始 body，而不是只保存网关返回结果。

### 7.3 账户信息必须隔离到管理接口

余额、组、价格和配额通常不属于 Chat 响应：

| 端点类别 | 典型字段 | 使用边界 |
|---|---|---|
| `/api/status` | 渠道、余额、用户状态 | 管理查询，不混入聊天响应 |
| `/api/quota` | 剩余配额、重置时间 | 限额控制 |
| `/api/dashboard` | 组织汇总、消费趋势 | 管理后台 |
| `/api/prices` 或模型配置 | 输入、输出、缓存价格 | 网关价格表来源 |
| `/v1/chat/completions` | `usage` | 仅返回请求用量 |

即使某个私有版本把余额塞入聊天响应，也应显式剥离。否则上游改动会直接破坏客户端 schema。

### 7.4 错误响应应统一包装，同时保留原始证据

| 来源 | 典型错误形状 | 归一化建议 |
|---|---|---|
| OpenAI | `{"error":{"message","type","code","param"}}`，HTTP 4xx/5xx | 映射为标准 `error`；保留 `type` |
| Anthropic | `{"type":"error","error":{"type,message}}`，常带 `request-id` | 保留 `request_id`；`type` 写入扩展字段 |
| Gemini | `error.code/status/message/details` | 映射为网关错误码；多 error 时保存数组 |
| 中转网关 | 可能返回上游错误、自定义 code 或 HTML | 先识别 HTTP，再解析 JSON；失败则保存 raw body |
| 本地后端 | 500、`{"detail":...}`、进程崩溃 | 标记为 `upstream_unstructured` |

通用归一化结构：

```json
{"error":{"code":"provider_error","message":"...","param":null},
 "provider_error":{"http_status":502,"raw":{...},"request_id":"..."}}
```

不要在错误发生时把 `usage=0` 误判为“请求没有消耗”。网络失败、流中断和后端错误都可能发生在用量已经产生之后。

## 8. 本地后端应先适配原生结构，再统一包装为标准 Chat

**本地后端最容易出现 token 字段缺失、标准字段由兼容层伪造，以及性能统计混入 usage。** 网关应按“原生协议优先、兼容层降级、网关估算兜底”的顺序处理。

| 后端 | 原生或兼容返回 | 缓存/推理字段 | 归一化动作 |
|---|---|---|---|
| Ollama 原生 `/api/generate`、`/api/chat` | `prompt_eval_count`、`eval_count`、`total_duration`、`load_duration` 等 | 本次官方证据未确认标准 Chat `usage` | 映射为 `input`、`output`；耗时进入 `perf`；不要伪造 `total_tokens` |
| Ollama `/v1` 兼容层 | 部分实现会构造 `prompt_tokens/completion_tokens/total_tokens` | `cached_tokens` 通常缺失 | 优先使用原生指标；记录兼容层版本 |
| vLLM OpenAI 兼容 | 标准 Chat usage | prefix caching、speculative decoding、logprobs 取决于版本和参数 | 从真实响应提取；不承诺固定字段 |
| SGLang OpenAI 兼容 | 标准 Chat usage | RadixAttention/prefix caching 可能存在；具体字段未确认 | 启用时记录 trace；缺失就留空 |
| LM Studio | 返回标准 usage，并附加 `stats`、`model_info` | 官方示例未见 `cached/reasoning` | usage 可直接映射；`stats` 进入性能字段 |
| llama.cpp server / llama-cpp-python | 可返回 `prompt_tokens/completion_tokens/total_tokens` | 本次证据未确认缓存/推理 details | 映射标准字段；缺失时按 tokenizer 估算并标记 |
| Xinference OpenAI 兼容 | 可返回 `-1` 表示未知 | 本次证据未确认 cache details | `-1`/`null` 进入 `missing`，不转成 `0` |
| Text Generation WebUI | 自建 API 或扩展 OpenAI 兼容 | 版本差异极大 | 逐 fork 录制快照；不预设 usage |
| LocalAI | OpenAI 兼容 | 取决于后端；可能缺失或估算 | 保存 backend 标签；估算值设置 `source=estimated` |

### 8.1 Ollama 的原生性能统计不应被包装成 token 细节

Ollama 官方 `/api/generate` 示例为：

```json
{
  "model": "gemma4",
  "created_at": "2025-10-17T23:14:07.414671Z",
  "response": "Hello!",
  "done": true,
  "done_reason": "stop",
  "total_duration": 174560334,
  "load_duration": 101397084,
  "prompt_eval_count": 11,
  "prompt_eval_duration": 13074791,
  "eval_count": 18,
  "eval_duration": 52479709
}
```

流式最终 chunk 也包含 usage 字段。[8] 映射规则应为：

```text
input  = prompt_eval_count
output = eval_count
perf.prompt_eval_duration_ns = prompt_eval_duration
perf.eval_duration_ns         = eval_duration
perf.total_duration_ns        = total_duration
```

这里没有原生缓存命中、写入或推理字段，因此不应伪造这些指标。

### 8.2 LM Studio 已接近标准 Chat，但扩展字段仍需隔离

官方 `POST /api/v0/chat/completions` 响应：

```json
{
  "usage": {"prompt_tokens":24,"completion_tokens":53,"total_tokens":77},
  "stats": {
    "tokens_per_second":51.437,
    "time_to_first_token":0.111,
    "generation_time":0.954,
    "stop_reason":"eosFound"
  },
  "model_info": {"arch":"granite","quant":"Q4_K_M","format":"gguf","context_length":4096}
}
```

标准 usage 可直接映射。`stats`、`model_info` 以及 `stop_reason` 与 OpenAI `finish_reason` 的差异，应放在扩展字段中。[9]

### 8.3 vLLM 与 SGLang 的能力取决于版本和启动参数

vLLM 可启动 OpenAI 兼容服务，但 prefix caching、speculative decoding 是否暴露 token 细节，取决于版本、启动参数和路由代码。SGLang 官方文档只确认其 OpenAI API 兼容性、RadixAttention 和 prefix caching 能力，本次未确认稳定的 `cached/rejected` usage 字段。[17][18]

网关可以采用以下配置策略：

```yaml
backend_capabilities:
  supports_prefix_cache_report: false
  supports_speculative_report: false
  usage_estimation_policy: tokenizer_fallback
```

如果真实响应后来新增字段，再通过补丁启用。不要因为上游实现了某项能力，就假定响应一定包含对应字段。

### 8.4 本地后端的估算值必须明确标注来源

当本地后端不返回 token 数时，可以按以下顺序估算：

1. 请求前使用模型对应 tokenizer 计算输入；
2. 流式中按增量 token 计数输出；
3. 请求后补齐 total。

但估算误差来自：

- 特殊 token；
- 图片、音频；
- 工具 schema；
- reasoning 是否可见；
- 不同 tokenizer 的字词切分差异。

因此，估算值只能用于：

- 限额预扣；
- 用户体验；
- 后台趋势分析。

不能直接用于精确收费。如果必须补写 usage，建议结构：

```json
"usage": {
  "prompt_tokens": 120,
  "completion_tokens": 34,
  "total_tokens": 154
},
"usage_metadata": {
  "source": "gateway_estimated",
  "tokenizer": "Qwen/tokenizer.json@rev",
  "estimation_error": "unknown"
}
```

计费应异步使用真实上游用量覆盖网关估算。

## 9. 缓存字段不能直接相加：核心陷阱是“包含还是独立”

**缓存字段的正确处理顺序是：先判断输入总量是否已包含命中部分，再决定是否扣除；创建量则必须来自独立字段。** 如果顺序颠倒，最常见结果是重复计费。

| 字段 | 是否通常包含在输入总量中 | 是否有独立创建量 | 网关处理 |
|---|---|---|---|
| OpenAI `prompt_tokens_details.cached_tokens` | 是，输入总量已包含 | Responses/Chat 通用接口未确认 | 读取时扣减；创建量不可推导 |
| Anthropic `cache_read_input_tokens` | 是，已包含在 `input_tokens` | 有，`cache_creation_input_tokens` | 分别记录 read 与 creation |
| DeepSeek `prompt_cache_hit_tokens` | 是，属于 `prompt_tokens` 的子集 | 文档未确认独立创建量 | 读取时扣减；创建量不可推导 |
| Gemini `cachedContentTokenCount` | 是，已包含在 `promptTokenCount` | 本次未确认独立创建字段 | 读取时扣减 |
| 火山方舟 `prompt_tokens_details.cached_tokens` | 按 OpenAI 语义，视为已包含 | 未确认 | 读取时扣减 |
| OpenRouter `prompt_tokens_details.cached_tokens` | 兼容层字段；精确口径按具体 provider 归一化 | 可选 `cache_write_tokens` | 优先使用 `cache_write_tokens` |

### 9.1 计费伪代码必须保留来源与边界

```python
def bill(u: CanonicalUsage, price: Price):
    input_for_billing = u.input_tokens
    cache_read = u.cache_read_tokens
    cache_create = u.cache_creation_tokens

    if u.contains_cache_read:
        input_for_billing = u.input_tokens - cache_read

    input_cost = (
        input_for_billing * price.input
        + cache_read * price.cache_read
        + cache_create * price.cache_create
    )
    output_cost = u.output_tokens * price.output
    return input_cost + output_cost
```

这一算法的成立条件包括：

- `contains_cache_read=true`；
- `cache_read` 确实是 `input_tokens` 的子集；
- `cache_create` 来自独立字段或得到上游明确支持。

如果条件不满足，应改用提供商原始响应计费，并标记 `billing_mode=raw`。

### 9.2 TTL 影响命中概率，不改变本次响应的基础字段口径

Anthropic 官方 `CacheControlEphemeral` 支持 `ttl=5m` 或 `1h`，并在 `cache_creation` 中按这两种 TTL 返回创建量。[3]

DeepSeek 只说明缓存为“尽力而为”，构建需要秒级时间，未使用后会自动清空，通常保留数小时至数天；本次未发现公开的最小前缀和精确 TTL 承诺。[11]

因此：

- 网关不应因为“5 分钟前成功命中”，就断言下一次必然命中；
- 计费应读取当前响应的实际 token，而不是根据 TTL 状态机推导；
- TTL 只用于解释命中变化，不应直接生成 usage。

## 10. 费用字段应按“提供商参考、网关结算”双轨保存

**绝大多数模型提供商不把可结算费用写入 Chat 响应。** 网关应保存上游金额作为参考，同时用受版本控制的本地价格表完成最终结算。

| API | 是否在响应中返回金额 | 字段与币种/精度 | 网关策略 |
|---|---|---|---|
| OpenAI Chat/Responses | 否 | 无 `cost` | 本地价格表；后台对账文件 |
| Anthropic Messages | 否 | 无 `cost` | 本地价格表；缓存 read/create 分价 |
| DeepSeek | 否 | 无 `cost` | 本地价格表；命中与未命中分价 |
| Google Gemini | 否 | `usageMetadata` 只有 token | 本地价格表 |
| 火山方舟/百炼等 | 通常否 | 价格主要在控制台/价格页 | 价格页不是响应字段 |
| OpenRouter | 是 | `usage.cost`、`cost_details`；文档写作 credits/USD 类金额 | 保存为参考；以网关表结算 |
| Groq、Together、Fireworks 等聚合层 | 视实现而定 | 本次未统一确认稳定 `cost` 字段 | 有则保存，不依赖 |
| New API/One API 类 | 余额不在 Chat 响应 | `/api/quota`、`/api/status` 等私有端点 | 管理 API 与聊天 API 隔离 |

费用字段应建模为：

```text
CostReport {
  provider_reported_cost: decimal | null
  provider_currency: string | null   // 如 "USD"、"credits"
  gateway_calculated_cost: decimal
  gateway_currency: string           // 统一内部币种
  pricing_version: string
  pricing_source: provider_price_api | local_table | estimated
}
```

精度使用十进制，不采用浮点；金额最小单位由内部账务决定。OpenRouter 的 `0.0012` 只能说明实际小数精度足够，不代表官方承诺固定精度。[15]

## 11. 缺失风险应按“无法结算、字段失真、体验降级”分级

**风险排序不能只看字段是否存在，还要看错误后果。** 导致重复计费或漏计费的字段为 P0；无法区分逻辑模型和真实模型为 P1；只影响 UX 的字段为 P2。

| 提供商/场景 | 常见缺失/失真 | 风险 | 网关应对 |
|---|---|---|---|
| OpenAI Chat 流式，未设置 `include_usage` | 只有末块有 usage，其他为 `null`；容易误认为 usage 全量 | P0 | 默认注入；缺失时标记 partial |
| OpenAI Chat 流中断 | 收不到末块 | P0 | 后台按 request_id 重查；不自动收费 |
| Responses 流式 | 事件类型多，`reasoning` 可能缺失 | P0 | 状态机累积；最终 response 优先 |
| Anthropic `message_start` | 可能只有部分输入 usage | P1 | 不早结算；等待 `message_stop` |
| Anthropic `message_delta` | `output_tokens` 累计覆盖；缓存字段可能只在一处出现 | P0 | 合并而非替换 |
| Anthropic→OpenAI 转换 | thinking、redacted、container、server_tool 丢失 | P1 | 保留 raw；thinking 使用扩展字段 |
| DeepSeek OpenAI | 旧/部分模型无 `prompt_cache_*` | P1 | 缺失不等于 0 |
| DeepSeek Anthropic | `cache_control` 被忽略 | P0，如按缓存计费 | 禁用或记录不支持；不生成缓存账单 |
| Gemini 原生 | OpenAI 兼容层未必暴露 modality、thoughts | P1 | 双适配器；不向下转换不存在的字段 |
| OpenRouter | `cost_details` 可能为 `null` | P2 | 只作展示/校验 |
| 国内兼容端点 | 不一定返回 `prompt_tokens_details` | P1 | 快照测试；禁止字段推测 |
| New API/One API | `model` 映射、usage 置 0/抹除、自定义错误 | P0 | 保存真实后端与 raw body |
| Ollama 原生 | 无标准 Chat usage | P1 | 使用原生字段构造规范 usage |
| vLLM/SGLang | cache/spec 细节取决于版本 | P1 | capability flag + 真实响应 |
| Xinference | 返回 `-1` | P0，如按 0 处理 | `-1` 映射为 missing |
| LocalAI/llama.cpp | tokenizer 估算误差、无 cache | P1 | 标记 estimated，不参与精确结算 |

风险等级定义：

- **P0：**可能错误计费、破坏协议或导致用量丢失，必须有测试与降级；
- **P1：**字段不可用，但可降级；
- **P2：**只影响 UX 或观测。

## 12. `finish_reason` 必须先归一到终止语义，再保留原始值

**不同协议的停止原因不能做纯字符串映射。** 网关应先判断终止类型，再保留原始值，避免把工具调用、截断、安全拦截全部误写成 `stop`。

| 语义 | OpenAI Chat | Anthropic Messages | Gemini | Ollama 原生 | 归一化 |
|---|---|---|---|---|---|
| 自然结束 | `stop` | `end_turn` | `STOP` | `stop` | `stop` |
| 达到 max tokens | `length` | `max_tokens` | `MAX_TOKENS` | `length` | `length` |
| 工具调用 | `tool_calls` | `tool_use` | `TOOL_CALL`/`FUNCTION_CALL` | 后端定义 | `tool_calls` |
| 命中 stop sequence | `stop` | `stop_sequence` | `STOP_SEQUENCE` | `stop` | `stop_sequence` |
| 内容安全 | `content_filter` | `refusal` | `SAFETY`/`RECITATION` | 后端定义 | `content_filter`/`refusal` |
| 上下文超限 | 后端定义 | `model_context_window_exceeded` | 错误/其他 | 后端定义 | `context_exceeded` |
| 暂停/继续 | OpenAI 通常无直接等价 | `pause_turn` | 后端定义 | 后端定义 | `pause` |

OpenAI 官方还列出已弃用的 `function_call`。在归一化响应中，应优先使用 `tool_calls`；原始 `function_call` 只保留在 `raw`。[1]

## 13. 网关落地应遵循“先录制、再映射、后结算”

**归一化层应分为不可跳过的五步：识别、转录、合并、校验、结算。** 任何一步都不能用默认值掩盖缺失，否则错误会从前置解析传播到计费。

### 13.1 五步处理链

1. **识别**

   根据以下信息确定适配器：

```text
provider = resolve(base_url, credentials, request_headers, response_shape)
```

禁止只根据请求路径判断。同一 `/v1/chat/completions` 可能对应数十种后端。

2. **转录**

将原始响应字段复制到 `raw_usage`，数值统一使用可空整数，不自动将 `null` 转换为 `0`。

3. **合并**

按“输入、输出、缓存读取、缓存创建、推理、音频”六类映射。流式响应通过状态机累积。

4. **校验**

校验规则包括：

- `total` 与各部分不一致：保留两者，标记 `consistency=warning`；
- 只存在 `cached`，不存在 `prompt`：保留，等待父对象；
- `reasoning` 明显大于 `output`：仅记录，不直接覆盖；
- 负值或 `-1`：标记 `missing`；
- 字符串数值：显式转换并记录解析日志。

5. **结算**

计费使用规范量；没有规范量时退化为原始量；两者都缺失时标记为 `pending`，不写成 0。

### 13.2 配置化的映射表优于长串 if/else

```yaml
providers:
  deepseek:
    protocol: openai_compatible
    usage_map:
      input: usage.prompt_tokens
      output: usage.completion_tokens
      cache_read: usage.prompt_cache_hit_tokens
      cache_creation: null
      reasoning: usage.completion_tokens_details.reasoning_tokens
    flags:
      contains_cache_read: true
      reasoning_in_output: true
    stream:
      final_usage_event: "chunk where choices=[] and usage present"
  anthropic:
    protocol: anthropic_messages
    usage_map:
      input: usage.input_tokens
      output: usage.output_tokens
      cache_read: usage.cache_read_input_tokens
      cache_creation: usage.cache_creation_input_tokens
      creation_ttl_breakdown: usage.cache_creation
    stream_merge:
      input_event: message_start
      output_event: message_delta
      final_event: message_stop
```

这套配置应同时包含：

- JSON 路径；
- 包含关系标志；
- 流式事件来源；
- 版本号；
- 最后验证日期；
- 真实响应快照引用。

### 13.3 容错解析应遵循缺失顺序

缺失时的兜底顺序为：

```text
1. 提供商原始明确值
2. 同响应中可推导且不改变计费边界的值
3. 网关 tokenizer 估算
4. 标记为 missing
```

例如：

- Anthropic 只返回 `input` 和 `output`，没有缓存细节：  
  `cache_read = 0` 会错误表达为“没有缓存”，应设为 `null`，并标记 `unknown`。
- OpenAI 返回 `prompt=100`、`cached=60`：  
  可直接计算 `non_cached_input=40`。
- 只有 `total`，没有 `prompt` 或 `completion`：  
  保留 total，其余字段留空。

数字类型解析规则：

```text
if value in (null, undefined): missing
if value is string and matches integer: parse with decimal/long
if value < 0 or > hard_limit: missing
```

### 13.4 tokenizer 估算只能用于非计费链路

推荐的估算流程：

1. 在网关内为每个模型注册 tokenizer；
2. 请求前计算输入；
3. 流式中统计增量输出；
4. 响应后补齐 total；
5. 如果后端随后返回真实 usage，异步覆盖估算值。

误差来源包括：

- 图像和音频 token；
- 工具 schema 编码；
- reasoning token 可见性；
- 多模态分块；
- BPE 差异。

因此：

- 限额预扣可使用估算值；
- 精确收费必须使用真实值或后台重算；
- 估算覆盖真实值前，应设置状态为 `provisional`。

### 13.5 流式中必须区分“内容完成”和“用量完整”

推荐状态：

```text
stream_finalized = content_complete && (final_usage_seen || non_recoverable_error)
usage_reliable    = final_usage_seen || raw_provider_usage_present
```

如果内容流已完成，但没有收到最终 usage，应：

- 将 usage 标记为 `partial`；
- 不立即写入计费；
- 触发后台对账；
- 仅在业务允许时使用估算值。

如果上游明确返回错误，应保留 `error`，不把已生成内容强行包装为成功，也不伪造 usage。

## 14. 最容易踩的 10 个坑

1. **把 OpenAI `cached_tokens` 再加回 `prompt_tokens`**  
   缓存命中的输入通常已经是 `prompt_tokens` 的子集，叠加后会重复计费。[1]

2. **假设 DeepSeek `prompt_cache_miss_tokens` 是缓存创建量**  
   官方确认的是命中与未命中，未确认独立创建量；不能推导写入费用。[11]

3. **假设 Anthropic `cache_read + cache_creation = input`**  
   Anthropic 的 `input_tokens` 通常包含读取部分；创建量是独立概念，并非未命中部分。[3]

4. **OpenAI Chat 流没有设置 `include_usage`**  
   除最终 chunk 外，`usage` 为 `null`，流中断时还可能收不到末块。[1]

5. **把 Anthropic `message_start.usage` 当成完整用量**  
   后续 `message_delta` 可能更新 `output_tokens` 及缓存字段，应持续合并。[5]

6. **把 Gemini `promptTokenCount` 当成纯未缓存输入**  
   官方明确说明，存在缓存内容时，该字段仍包含缓存部分。[4]

7. **把中转网关返回的 `model` 当作真实后端模型**  
   逻辑名、渠道映射键和真实模型可能不同；计费与审计必须使用 `model_real`。[6][7]

8. **把 `-1`、`null`、缺失字段统一转为 `0`**  
   Xinference 的 `-1` 和未返回的字段都表示未知，转 0 会伪造用量。[10]

9. **把 Ollama 的 `prompt_eval_count`/`eval_count` 误当成标准 usage**  
   这是原生统计；通过 `/v1` 兼容层转发时，还应检查是否伪造了 `total_tokens`。[8]

10. **直接使用上游 `cost` 进行内部结算**  
   除 OpenRouter 等少数平台外，大多数官方端点不返回费用。内部账务应依赖版本化价格表、原始用量和复核流程。[15]

## 引用来源

[1] https://platform.openai.com/docs/api-reference/chat/object
> “An optional field that will only be present when you set stream_options: {"include_usage": true} in your request. When present, it contains a null value except for the last chunk which contains the token usage statistics for the entire request.”

[2] https://platform.openai.com/docs/guides/reasoning
> “The exact number of reasoning tokens used is visible in the usage object of the response object, under output_tokens_details.”

[3] https://docs.anthropic.com/en/api/messages
> “Usage object{ cache_creation, cache_creation_input_tokens, cache_read_input_tokens, 6 more }”；“CacheControlEphemeral object{ type, ttl } ... ttl: optional '5m' or '1h'”.

[4] https://ai.google.dev/api/generate-content
> “When cachedContent is set, this is still the total effective prompt size meaning this includes the number of tokens in the cached content.”

[5] https://docs.anthropic.com/en/api/messages-streaming
> “Each stream uses the following event flow: 1. message_start ... 2. A series of content blocks ... 3. One or more message_delta events ... 4. A final message_stop event.”

[6] https://github.com/songquanpeng/one-api
> One API 相关仓库 README、issue 与源码应在具体部署版本中复核；本次未发现足以确认稳定 usage 透传的官方规范。

[7] https://github.com/Calcium-Ion/new-api
> “Authorized Usage Accounting and Billing: ... organization-level per-request, usage-based, and cache-hit cost accounting.”

[8] https://docs.ollama.com/api/usage
> “prompt_eval_count: How many input tokens were processed”；“eval_count: How many output tokens were processed”.

[9] https://lmstudio.ai/docs/developer/rest/endpoints
> “POST /api/v0/chat/completions”；“usage”: {"prompt_tokens": 24, "completion_tokens": 53, "total_tokens": 77}.

[10] https://inference.readthedocs.io/en/v1.1.1/getting_started/using_xinference.html
> “usage”: {"prompt_tokens": -1, "completion_tokens": -1, "total_tokens": -1}.

[11] https://api-docs.deepseek.com/zh-cn/guides/kv_cache/
> “prompt_cache_hit_tokens: 本次请求的输入中，缓存命中的 tokens 数”；“prompt_cache_miss_tokens: 本次请求的输入中，缓存未命中的 tokens 数”.

[12] https://api-docs.deepseek.com/guides/anthropic_api
> “cache_control Ignored”；“container Ignored”；“service_tier Ignored”.

[13] https://volcengine.com/docs/6517/1529329
> “usage.prompt_tokens_details.cached_tokens”；“usage.completion_tokens_details.reasoning_tokens”.

[14] https://openrouter.ai/docs/api-reference/chat-completion
> “prompt_tokens_details?:{ cached_tokens: number; cache_write_tokens?: number; audio_tokens?: number; video_tokens?: number }”.

[15] https://openrouter.ai/docs/client-sdks/python/components/generationresponsedata
> “cost?: number”；“upstream_inference_cost?: number”；“total_cost float: Total cost of the generation in USD”.

[16] https://github.com/Calcium-Ion/new-api
> “OpenAI Compatible ⇄ Claude Messages”；“Cache billing statistics for OpenAI, Azure, DeepSeek, Claude, Qwen, and supported models”.

[17] https://docs.sglang.io/
> “Compatible with Hugging Face and OpenAI APIs”；“RadixAttention, prefix caching, and multi-GPU parallelism”.

[18] https://qwen.readthedocs.io/zh-cn/latest/deployment/vllm.html
> “借助vLLM，构建一个与OpenAI API兼容的API服务十分简便.”

[19] https://api-docs.deepseek.com/api/create-chat-completion?source=post_page-----bc3cbfd8d841---------------------------------------
> “reasoning_content object[]”；“usage object ... prompt_cache_hit_tokens ... prompt_cache_miss_tokens ... completion_tokens_details { reasoning_tokens }”.

[20] https://api-docs.deepseek.com/zh-cn
> “DeepSeek API 使用与 OpenAI/Anthropic 兼容的 API 格式.”

[21] https://docs.litellm.ai/docs/completion/prompt_caching
> “For the supported providers, LiteLLM follows the OpenAI prompt caching usage object format.”

[22] https://platform.openai.com/docs/api-reference/responses
> 完整 Responses API 文档索引及 Markdown 版本说明；“For the complete documentation index, see llms.txt.”

[23] https://platform.openai.com/docs/api-reference/chat/streaming
> OpenAI API 概览页包含流式事件和向后兼容变更说明；“Adding new event types in streaming APIs.”

[24] https://dashscope.aliyun.com/?ref=ainav.cn/
> 阿里云百炼定价页：“显式缓存创建”“显式缓存命中”与不同输入档位价格。

[25] https://docs.siliconflow.cn/cn/userguide/capabilities/text-generation
> “您可以通过 OpenAI SDK进行端到端接口请求”；本次摘录未确认缓存 usage 字段。

[26] https://leeroopedia.com/index.php?oldid=1463&title=Workflow:Helicone_Helicone_LLM_Response_Normalization
> “Anthropic returns usage in a top-level usage object with input_tokens/output_tokens ... OpenAI returns usage in usage.prompt_tokens/completion_tokens.”