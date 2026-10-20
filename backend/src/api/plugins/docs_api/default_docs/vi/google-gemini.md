# Hướng dẫn API Gốc Google Gemini

Nếu bạn sử dụng SDK chính thức của Google hoặc muốn bỏ qua lớp chuyển đổi giao thức để sử dụng Multi-modal, System Instruction hoặc JSON Mode gốc của Gemini, bạn có thể gọi trực tiếp định tuyến Google gốc của Gateway.

### 1. Tạo văn bản (Non-stream)
* **Đường dẫn**: `/v1beta/models/{model}:generateContent`
* **Phương thức yêu cầu**: `POST`

### 2. Tạo luồng (Streaming)
* **Đường dẫn**: `/v1beta/models/{model}:streamGenerateContent`
* **Phương thức yêu cầu**: `POST`

#### Ví dụ dữ liệu yêu cầu (Payload) cốt lõi
```json
{
  "contents": [
    {
      "role": "user",
      "parts": [
        {
          "text": "请扮演我的私人旅行助手，规划一份 3 天的京都赏樱路线。"
        }
      ]
    }
  ],
  "systemInstruction": {
    "parts": [
      {
        "text": "你是一个专业的旅行规划师，语气亲切幽默。"
      }
    ]
  },
  "generationConfig": {
    "temperature": 0.4,
    "maxOutputTokens": 2000,
    "responseMimeType": "text/plain"
  }
}
```

### 3. Giao diện tương tác thống nhất Interactions (v1 / v1beta)
* **Đường dẫn**: `/v1/interactions` (Tương thích `/v1beta/interactions`)
* **Phương thức yêu cầu**: `POST`

Giao diện tương tác thống nhất Interactions hoàn toàn mới của Google Gemini, hỗ trợ chỉ định trực tiếp `model` (hoặc `agent`) cùng với `input` (văn bản, đa phương thức hoặc tương tác nhiều lượt), đồng thời hỗ trợ `system_instruction`, `tools`, `response_format`, v.v.

#### Ví dụ dữ liệu yêu cầu (Payload) cốt lõi
```json
{
  "model": "gemini-2.5-flash",
  "input": "Hãy giúp tôi lên lịch trình ngắm hoa anh đào 3 ngày tại Kyoto.",
  "system_instruction": "Bạn là một chuyên gia lập kế hoạch du lịch chuyên nghiệp, thân thiện và hài hước.",
  "stream": false
}
```

#### Phương thức xác thực (Chọn 1 trong 3)
* Header tiêu chuẩn: `Authorization: Bearer sk-your_token`
* Header Google: `X-Goog-Api-Key: sk-your_token`
* Tham số ở cuối URL: `?key=sk-your_token`
