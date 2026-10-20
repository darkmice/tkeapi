# Giao diện Tạo Video

API video bất đồng bộ tương thích OpenAI. Video Wan (`wan3.0-video`, `files`/`links`): xem **`wan-video`**.

### 1. Gửi Nhiệm vụ Video
* **Đường dẫn**: `/v1/video/generations`
* **Phương thức Yêu cầu**: `POST`

#### Giải thích các tham số cốt lõi
| Tên tham số | Kiểu dữ liệu | Bắt buộc | Mô tả |
| :--- | :--- | :--- | :--- |
| `model` | `string` | Có | Tên mô hình tạo video, ví dụ `doubao-seedance-2-5`, `kling-v3-omni`, `wan3.0-video` |
| `prompt` | `string` | Có | Văn bản câu lệnh gợi ý (prompt) mô tả chuyển động và hình ảnh của video |
| `negative_prompt` | `string` | Không | Câu lệnh gợi ý phủ định (negative prompt) để loại bỏ các yếu tố hình ảnh không mong muốn |
| `images` / `image_urls` | `array` | Không | Mảng chứa liên kết hoặc dữ liệu base64 của hình ảnh tham chiếu (`image_urls` có hiệu lực hoàn toàn giống `images`). Một ảnh làm khung hình đầu, hai ảnh làm khung hình đầu/cuối, ba ảnh trở lên làm tham chiếu nhiều ảnh (như Kling, Volcengine) |
| `videos` | `array` | Không | Mảng liên kết video tham chiếu cho chế độ video-to-video hoặc điều khiển video (ví dụ: Kling Omni, Bytefor) |
| `audios` | `array` | Không | URL audio tham chiếu |
| `files` | `array` | Không | URL file tham chiếu (Wan; role mặc định `file`) |
| `links` | `array` | Không | URL trang web tham chiếu (Wan; role mặc định `link`) |
| `resolution` | `string` | Không | Độ phân giải (vd. `1080p`, `720p`) |
| `ratio` | `string` | Không | Tùy chọn tỷ lệ khung hình (ví dụ `16:9`, `9:16`, `4:3`, `3:4`, `1:1`) |
| `duration` | `integer` | Không | Thời lượng video được tạo (giây), ví dụ `5` hoặc `10`. Với Jimeng AI, thời lượng được tự động chuyển đổi thành số khung hình `121` or `241` |
| `generate_audio` | `boolean` | Không | Đồng bộ tạo hiệu ứng âm thanh nền/lồng tiếng (mặc định là `false`) |
| `watermark` | `boolean` | Không | Có thêm hình mờ (watermark) vào video hay không (hỗ trợ bởi Volcengine, Alibaba, v.v.) |
| `web_search` | `boolean` | Không | Tìm kiếm web (mặc định `false`; Volcengine/Gemini; Wan bỏ qua) |
| `seed` | `integer` | Không | Hạt giống ngẫu nhiên (dùng để kiểm soát tính nhất quán của video được tạo) |

#### Ví dụ gửi nhiệm vụ
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

#### Phản hồi gửi nhiệm vụ (Nhận Task ID)
```json
{
  "id": "video_task_abc123xyz789",
  "task_id": "video_task_abc123xyz789",
  "status": "pending",
  "message": "Task submitted successfully"
}
```

### 2. Vòng lặp lấy kết quả nhiệm vụ (Polling)
* **Đường dẫn**: `/v1/video/generations/{task_id}` hoặc `/v1/tasks/{task_id}`
* **Phương thức Yêu cầu**: `GET`

#### Ví dụ phản hồi truy vấn (Tạo thành công)
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

### 3. Hủy nhiệm vụ video
* **Đường dẫn**: `/v1/video/generations/{task_id}` hoặc `/v1/tasks/{task_id}`
* **Phương thức Yêu cầu**: `DELETE`
* **Ghi chú**: Hủy nhiệm vụ chưa hoàn thành. Hàng đợi ComfyUI trên nền tảng sẽ ngắt đúng nhiệm vụ trên node và hoàn tiền tạm khóa. Với Volcengine, dùng `DELETE /api/v3/contents/generations/tasks/{task_id}`.

```bash
curl -X DELETE https://{{domain}}/v1/video/generations/video_task_abc123xyz789 \
  -H "Authorization: Bearer sk-your_token"
```

