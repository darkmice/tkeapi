# Kling AI ネイティブプロトコル説明

Kling AI は、非常に高い水準の動画画質とモーションコントロールにより、広く使用されています。ゲートウェイは Kling 専用のルーティングを用意し、Kling 公式 API と完全に整合させています。 

### 1. 動画モデル API (公式ネイティブ)
* **テキストからの動画生成**: `/text-to-video/{model_id}` (`POST`)
* **画像からの動画生成**: `/image-to-video/{model_id}` (`POST`)
* **Omni 動画参照からの動画生成**: `/omni-video/{model_id}` (`POST`)
* **タスクステータス確認**: `/tasks?task_ids={task_id}` (`GET`)

### 2. 画像モデル API
* **標準テキスト/画像からの画像生成**: `/v1/images/generations` (`POST`)
* **複数画像からの画像生成**: `/v1/images/multi-image2image` (`POST`)
* **Omni 画像生成**: `/v1/images/omni-image` (`POST`)
* **タスクステータス確認**: `/v1/images/{endpoint}/{task_id}` (`GET`)

### 3. Kling 公式ドキュメントの参照
詳細なリクエストペイロード構造（例: `camera_control` カメラ制御、`aspect_ratio` アスペクト比制御、最初/最後のフレーム画像など）については、公式標準をご参照ください。以下のリンクから公式ドキュメントにアクセスできます：
* [Kling OmniVideo 公式仕様](https://klingai.com/document-api/apiReference/model/OmniVideo)
* [Kling OmniImage 公式仕様](https://klingai.com/document-api/apiReference/model/OmniImage)
