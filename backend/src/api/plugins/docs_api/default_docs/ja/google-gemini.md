# Google Gemini ネイティブ API 説明

Google SDK または Gemini 形式を使う場合は、下記パスと本プラットフォームの API キーで呼び出してください。

### 1. テキスト生成 (Non-stream)
* **パス**: `/v1beta/models/{model}:generateContent`
* **リクエストメソッド**: `POST`

### 2. ストリーミング生成 (Streaming)
* **パス**: `/v1beta/models/{model}:streamGenerateContent`
* **リクエストメソッド**: `POST`

#### 主要リクエストペイロード例
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

### 3. Interactions 統合インターフェース (v1 / v1beta)
* **パス**: `/v1/interactions` (`/v1beta/interactions` と互換)
* **リクエストメソッド**: `POST`

Google Gemini の新しい統合インタラクション API。`model` (または `agent`) と `input` を直接指定でき、`system_instruction`、`tools`、`response_format` などのパラメータをサポートします。

#### 主要リクエストペイロード例
```json
{
  "model": "gemini-2.5-flash",
  "input": "京都の3日間の桜観光プランを作成してください。",
  "system_instruction": "あなたは親切でユーモアのあるプロの旅行プランナーです。",
  "stream": false
}
```

#### 認証方法（以下のいずれか1つ）
* 標準ヘッダー: `Authorization: Bearer sk-your_token`
* Google ヘッダー: `X-Goog-Api-Key: sk-your_token`
* URL パラメータ末尾: `?key=sk-your_token`
