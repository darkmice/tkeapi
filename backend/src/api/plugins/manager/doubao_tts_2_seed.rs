/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia
 * @license        MIT (https://www.tkeapi.com/)
 */

//! 创作中心2026：Doubao 语音合成 2.0 音频方案。
//! 仅 playground_2026 注入。参数对齐本站 `POST /v1/audio/speech`（OpenAI 兼容）
//! 及火山 TTS V3 转换（voice→speaker，response_format/sample_rate/speed→audio_params）。
//! https://www.volcengine.com/docs/6561/1598757
//! 网关文档：`POST /v1/audio/speech`，model=`seed-tts-2.0`。

use serde_json::{json, Value};

pub const DOUBAO_TTS_2_SCHEME_ID: &str = "doubao_tts_2_0";

/// 文生语音：提示词 → 音频。TTS 2.0 不走声音复刻参考音。
pub fn doubao_tts_2_scheme_io() -> Value {
    json!({
        "inputs": [
            {
                "key": "prompt",
                "label": "提示词",
                "enabled": true,
                "modality": "text",
                "handle_prefix": "Prompt",
                "bind_key": "prompt",
                "accepts": ["prompt"],
                "required": true,
                "max": 1
            }
        ],
        "outputs": [
            {
                "key": "audio",
                "label": "音频",
                "enabled": true,
                "modality": "audio",
                "handle_prefix": "Audio",
                "result_key": "audio_url",
                "max": 1
            }
        ]
    })
}

pub fn doubao_tts_2_scheme() -> Value {
    let io = doubao_tts_2_scheme_io();
    let mut scheme = json!({
        "id": DOUBAO_TTS_2_SCHEME_ID,
        "name": "Doubao 语音合成 2.0 方案",
        "type": "audio",
        "is_system": true,
        "endpoint": "/v1/audio/speech",
        "max_reference_images": 0,
        "description": "豆包语音合成 2.0（seed-tts-2.0）：文稿经本站 /v1/audio/speech 合成语音。音色走方案/模型「音色」专有配置；输出格式/采样率/语速对应官方 audio_params。语速按火山 speech_rate（-50～100，0 为正常）。",
        "voice_library": {
            "enabled": true,
            "catalog": "doubao_tts_2_0",
            "default": "zh_female_vv_uranus_bigtts"
        },
        "params": [
            {
                "key": "response_format",
                "label": "输出格式",
                "type": "select",
                "data_type": "string",
                "options": ["mp3", "wav", "pcm", "ogg_opus"],
                "option_labels": {
                    "mp3": "MP3",
                    "wav": "WAV",
                    "pcm": "PCM",
                    "ogg_opus": "Ogg Opus"
                },
                "default": "mp3",
                "hint": "对应官方 audio_params.format；网关默认 mp3"
            },
            {
                "key": "sample_rate",
                "label": "采样率",
                "type": "select",
                "data_type": "integer",
                "options": [16000, 24000, 44100, 48000],
                "option_labels": {
                    "16000": "16 kHz",
                    "24000": "24 kHz",
                    "44100": "44.1 kHz",
                    "48000": "48 kHz"
                },
                "default": 24000,
                "hint": "对应官方 audio_params.sample_rate，默认 24000"
            },
            {
                "key": "speed",
                "label": "语速",
                "type": "slider",
                "data_type": "integer",
                "min": -50,
                "max": 100,
                "step": 1,
                "default": 0,
                "hint": "网关将 speed 写入火山 speech_rate：0 正常，100 为 2 倍，-50 为 0.5 倍"
            }
        ]
    });
    if let Some(obj) = scheme.as_object_mut() {
        if let Some(inputs) = io.get("inputs").cloned() {
            obj.insert("inputs".into(), inputs);
        }
        if let Some(outputs) = io.get("outputs").cloned() {
            obj.insert("outputs".into(), outputs);
        }
    }
    scheme
}

pub fn seed_doubao_tts_2_scheme(schemes: &mut Vec<Value>) {
    let next = doubao_tts_2_scheme();
    if let Some(pos) = schemes
        .iter()
        .position(|s| s.get("id").and_then(|v| v.as_str()) == Some(DOUBAO_TTS_2_SCHEME_ID))
    {
        schemes[pos] = next;
    } else {
        schemes.push(next);
    }
}
