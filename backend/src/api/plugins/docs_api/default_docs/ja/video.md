# ビデオ生成インターフェース

OpenAI 互換の非同期動画 API `/v1/video/generations`。万相動画（`wan3.0-video`、`files`/`links`）は **`wan-video`** を参照。

### 1. ビデオタスクの送信
* **パス**: `/v1/video/generations`
* **リクエスト方式**: `POST`

#### 主要パラメータの説明
| パラメータ名 | タイプ | 必須 | 説明 |
| :--- | :--- | :--- | :--- |
| `model` | `string` | はい | ビデオ生成モデル名。例：`doubao-seedance-2-5`, `kling-v3-omni`, `wan3.0-video` |
| `prompt` | `string` | はい | ビデオの動きや映像を記述するテキストプロンプト |
| `negative_prompt` | `string` | いいえ | ネガティブプロンプト。不要な画像要素を回避するために使用します |
| `images` / `image_urls` | `array` | いいえ | 参照画像URLまたはbase64の配列（`image_urls`と`images`は全く同じ効果です）。1枚は開始フレーム、2枚は開始・終了フレーム、3枚以上はマルチ画像参照として使用可能（可霊/火山など） |
| `videos` | `array` | いいえ | 参照ビデオリンクの配列。ビデオ生成ビデオやビデオコントロールに使用可能（例：可霊Omniビデオ参照/Byteforビデオ参照） |
| `audios` | `array` | いいえ | 参照オーディオリンクの配列 |
| `files` | `array` | いいえ | 参照ファイル URL（万相；既定 role `file`） |
| `links` | `array` | いいえ | 参照 Web ページ URL（万相；既定 role `link`） |
| `resolution` | `string` | いいえ | 目標解像度（例：`1080p`, `720p`, `480p`） |
| `ratio` | `string` | いいえ | アスペクト比オプション（例：`16:9`, `9:16`, `4:3`, `3:4`, `1:1`）。システムは自動的にメーカー仕様パラメータに変換します |
| `duration` | `integer` | いいえ | 生成するビデオの長さ（秒）。例：`5` または `10`。即夢AIでは自動的にフレーム数 `121` または `241` に変換されます |
| `generate_audio` | `boolean` | いいえ | ビデオに背景音/ナレーションを同期生成するかどうか（デフォルトは `false`） |
| `watermark` | `boolean` | いいえ | 生成されたビデオにウォーターマークを追加するかどうか（火山方舟、アリ百煉などの一部のチャネルでサポート） |
| `web_search` | `boolean` | いいえ | ネット検索（デフォルト `false`；火山/Gemini 等、万相は無視） |
| `seed` | `integer` | いいえ | 乱数シード（ビデオ生成の一貫性を制御するために使用） |

#### タスク送信例
```bash
curl -X POST https://{{domain}}/v1/video/generations \
  -H "Authorization: Bearer sk-your_token" \
  -H "Content-Type: application/json" \
  -d '{
    "model": "doubao-seedance-2-5-260628",
    "prompt": "ヨーロッパ風の古典的な扉を押し開けると、目の前に幻想的な雲海の城が広がる。ドローン撮影の視点、4kレベルの詳細",
    "resolution": "1080p",
    "duration": 5
  }'
```

#### 送信レスポンス (Task IDの取得)
```json
{
  "id": "video_task_abc123xyz789",
  "task_id": "video_task_abc123xyz789",
  "status": "pending",
  "message": "Task submitted successfully"
}
```

### 2. ポーリングによるタスク結果の取得
* **パス**: `/v1/video/generations/{task_id}` または `/v1/tasks/{task_id}`
* **リクエスト方式**: `GET`

#### 照会レスポンス例 (生成成功)
```json
{
  "id": "video_task_abc123xyz789",
  "task_id": "video_task_abc123xyz789",
  "status": "completed",
  "data": [
    {
      "url": "https://example.com/output/generated_video.mp4"
    }
  ]
}
```

### 3. ビデオタスクのキャンセル
* **パス**: `/v1/video/generations/{task_id}` または `/v1/tasks/{task_id}`
* **リクエスト方式**: `DELETE`
* **説明**: 未完了のタスクをキャンセルします。本プラットフォームの ComfyUI キューはノード上の当該タスクを中断し、凍結済みの前払いを返金します。火山方舟はネイティブの `DELETE /api/v3/contents/generations/tasks/{task_id}` を使用してください。

```bash
curl -X DELETE https://{{domain}}/v1/video/generations/video_task_abc123xyz789 \
  -H "Authorization: Bearer sk-your_token"
```

