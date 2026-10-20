# Google Gemini 네이티브 API 설명

Google SDK 또는 Gemini 형식을 사용하는 경우, 아래 경로와 본 플랫폼 API Key로 호출하세요.

### 1. 텍스트 생성 (Non-stream)
* **경로**: `/v1beta/models/{model}:generateContent`
* **요청 메서드**: `POST`

### 2. 스트리밍 생성 (Streaming)
* **경로**: `/v1beta/models/{model}:streamGenerateContent`
* **요청 메서드**: `POST`

#### 핵심 요청 페이로드 예시
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

### 3. Interactions 통합 인터페이스 (v1 / v1beta)
* **경로**: `/v1/interactions` (`/v1beta/interactions` 호환)
* **요청 메서드**: `POST`

Google Gemini의 새로운 통합 인터랙션 API로, `model`(또는 `agent`) 및 `input`을 직접 지정할 수 있으며 `system_instruction`, `tools`, `response_format` 등의 매개변수를 지원합니다.

#### 핵심 요청 페이로드 예시
```json
{
  "model": "gemini-2.5-flash",
  "input": "교토 3일 벚꽃 여행 코스를 계획해 주세요.",
  "system_instruction": "당신은 친절하고 유머러스한 전문 여행 플래너입니다.",
  "stream": false
}
```

#### 인증 방식 (택일)
* 표준 헤더: `Authorization: Bearer sk-your_token`
* Google 헤더: `X-Goog-Api-Key: sk-your_token`
* URL 파라미터: `?key=sk-your_token`
