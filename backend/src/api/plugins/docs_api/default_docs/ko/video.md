# 비디오 생성 인터페이스

OpenAI 호환 비동기 영상 API. 알리万相 영상(`wan3.0-video`, `files`/`links`)은 **`wan-video`** 참조.

### 1. 비디오 작업 제출
* **경로**: `/v1/video/generations`
* **요청 방식**: `POST`

#### 핵심 매개변수 설명
| 매개변수명 | 타입 | 필수 여부 | 설명 |
| :--- | :--- | :--- | :--- |
| `model` | `string` | 예 | 비디오 생성 모델 이름 (예: `doubao-seedance-2-5`, `kling-v3-omni`, `wan3.0-video`) |
| `prompt` | `string` | 예 | 비디오의 움직임과 화면을 묘사하는 프롬프트 텍스트 |
| `negative_prompt` | `string` | 아니오 | 부정적 프롬프트, 원치 않는 화면 요소를 배제하는 데 사용 |
| `images` / `image_urls` | `array` | 아니오 | 참조용 베이스 이미지 URL 또는 base64 배열 (`image_urls`와 `images`는 완전히 동일하게 작동). 1장은 첫 프레임, 2장은 첫/마지막 프레임, 3장 이상은 멀티 이미지 참조로 사용 가능 (Kling, Volcengine 등) |
| `videos` | `array` | 아니오 | 참조용 비디오 링크 배열, 비디오-투-비디오 또는 비디오 제어용 (예: Kling Omni 비디오 참조/Bytefor 비디오 참조) |
| `audios` | `array` | 아니오 | 참조 오디오 URL |
| `files` | `array` | 아니오 | 참조 파일 URL（万相；기본 role `file`） |
| `links` | `array` | 아니오 | 참조 웹페이지 URL（万相；기본 role `link`） |
| `resolution` | `string` | 아니오 | 목표 해상도 (예: `1080p`, `720p`) |
| `ratio` | `string` | 아니오 | 가로세로 비율 옵션 (예: `16:9`, `9:16`, `4:3`, `3:4`, `1:1`). 시스템이 자동으로 제조사 매개변수로 변환합니다 |
| `duration` | `integer` | 아니오 | 생성할 비디오 길이 (초) (예: `5` 또는 `10`). 즉몽(Jimeng) AI에서는 자동으로 `121` 또는 `241` 프레임으로 변환됩니다 |
| `generate_audio` | `boolean` | 아니오 | 비디오 배경음/나레이션 오디오의 동시 생성 여부 (기본값 `false`) |
| `watermark` | `boolean` | 아니오 | 생성된 비디오에 워터마크 표시 여부 (Volcengine, Alibaba 등 일부 채널 지원) |
| `web_search` | `boolean` | 아니오 | 웹 검색 (기본 `false`; Volcengine/Gemini 등, 万相 무시) |
| `seed` | `integer` | 아니오 | 난수 시드 (비디오 생성의 일관성을 제어하기 위해 사용) |

#### 작업 제출 예시
```bash
curl -X POST https://{{domain}}/v1/video/generations \
  -H "Authorization: Bearer sk-your_token" \
  -H "Content-Type: application/json" \
  -d '{
    "model": "doubao-seedance-2-5-260628",
    "prompt": "推开欧式古典大门，展现在眼前的是一片奇幻的云海城堡，航拍视角，4k 级细节",
    "resolution": "1080p",
    "duration": 5
  }'
```

#### 제출 응답 (Task ID 획득)
```json
{
  "id": "video_task_abc123xyz789",
  "task_id": "video_task_abc123xyz789",
  "status": "pending",
  "message": "Task submitted successfully"
}
```

### 2. 폴링을 통한 작업 결과 획득
* **경로**: `/v1/video/generations/{task_id}` 또는 `/v1/tasks/{task_id}`
* **요청 방식**: `GET`

#### 조회 응답 예시 (생성 성공)
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

### 3. 비디오 작업 취소
* **경로**: `/v1/video/generations/{task_id}` 또는 `/v1/tasks/{task_id}`
* **요청 방식**: `DELETE`
* **설명**: 아직 끝나지 않은 작업을 취소합니다. 본 플랫폼 ComfyUI 큐는 노드에서 해당 작업을 중단하고 동결된 선차감을 환불합니다. 볼케이노 엔진은 네이티브 `DELETE /api/v3/contents/generations/tasks/{task_id}`를 사용하세요.

```bash
curl -X DELETE https://{{domain}}/v1/video/generations/video_task_abc123xyz789 \
  -H "Authorization: Bearer sk-your_token"
```

