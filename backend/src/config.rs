/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia
 * @license        MIT (https://www.tkeapi.com/)
 */

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub host: String,
    pub port: u16,
    pub database_url: String,
    pub jwt_secret: String,
    pub encryption_key: String,
    pub default_user_quota: f64,
    pub register_enabled: bool,
    /// 站点域名（OAuth 回调等场景使用）
    pub base_url: String,
    /// 数据目录路径（存储图标、素材等静态文件）
    pub data_dir: String,
    /// 门户静态文件保存目录
    pub portal_dir: String,
    /// 站点门户增强版静态文件保存目录（商业插件，与 portal_dir 隔离）
    pub portal_pro_dir: String,
    /// 图标和素材的发布目录路径
    pub assets_dir: String,
}

impl AppConfig {
    pub fn from_env() -> Self {
        let data_dir = std::env::var("DATA_DIR").unwrap_or_else(|_| "data".to_string());
        // 确保数据目录存在，以便写入持久化生成的安全凭证文件
        let _ = std::fs::create_dir_all(&data_dir);

        // 优先从持久化配置文件中读取数据库连接 URL (实现后台动态切换后持久化生效)
        let db_url_file = format!("{}/.database_url", data_dir);
        let database_url = if let Ok(content) = std::fs::read_to_string(&db_url_file) {
            let trimmed = content.trim().to_string();
            if !trimmed.is_empty() {
                trimmed
            } else {
                std::env::var("DATABASE_URL").unwrap_or_else(|_| {
                    "postgres://tokensapi:tokensapi@localhost:5432/tokensapi".to_string()
                })
            }
        } else {
            std::env::var("DATABASE_URL").unwrap_or_else(|_| {
                "postgres://tokensapi:tokensapi@localhost:5432/tokensapi".to_string()
            })
        };

        let portal_dir =
            std::env::var("PORTAL_DIR").unwrap_or_else(|_| format!("{}/portal", data_dir));
        // 确保门户静态目录存在
        let _ = std::fs::create_dir_all(&portal_dir);

        let portal_pro_dir =
            std::env::var("PORTAL_PRO_DIR").unwrap_or_else(|_| format!("{}/portal_pro", data_dir));
        let _ = std::fs::create_dir_all(&portal_pro_dir);

        let assets_dir =
            std::env::var("ASSETS_DIR").unwrap_or_else(|_| format!("{}/assets", data_dir));
        // 确保素材发布目录存在
        let _ = std::fs::create_dir_all(&assets_dir);

        // 1. JWT_SECRET：优先环境变量（排除示例模板值与小于16位的弱值），否则从文件加载或自动生成
        let jwt_secret = std::env::var("JWT_SECRET")
            .ok()
            .filter(|s| {
                let t = s.trim();
                t.len() >= 16 && !t.contains("change-me")
            })
            .unwrap_or_else(|| {
                let secret_file = format!("{}/.jwt_secret", data_dir);
                if let Ok(content) = std::fs::read_to_string(&secret_file) {
                    let trimmed = content.trim();
                    if !trimmed.is_empty() {
                        return trimmed.to_string();
                    }
                }
                let generated: String = (0..32)
                    .map(|_| {
                        let r: u8 = rand::random();
                        format!("{:02x}", r)
                    })
                    .collect();
                if let Err(e) = std::fs::write(&secret_file, &generated) {
                    eprintln!(
                        "⚠️ [ERROR] Failed to save generated JWT_SECRET to {}: {}",
                        secret_file, e
                    );
                }
                generated
            });

        // 2. ENCRYPTION_KEY：如端点未配置，在数据目录下生成并持久化 32 字节 Hex 强密钥
        let encryption_key = std::env::var("ENCRYPTION_KEY").unwrap_or_else(|_| {
            let key_file = format!("{}/.encryption_key", data_dir);
            if let Ok(content) = std::fs::read_to_string(&key_file) {
                let trimmed = content.trim();
                if !trimmed.is_empty() {
                    return trimmed.to_string();
                }
            }
            let generated: String = (0..32)
                .map(|_| {
                    let r: u8 = rand::random();
                    format!("{:02x}", r)
                })
                .collect();
            if let Err(e) = std::fs::write(&key_file, &generated) {
                eprintln!(
                    "⚠️ [ERROR] Failed to save generated ENCRYPTION_KEY to {}: {}",
                    key_file, e
                );
            }
            generated
        });

        Self {
            host: std::env::var("HOST").unwrap_or_else(|_| "0.0.0.0".to_string()),
            port: std::env::var("PORT")
                .unwrap_or_else(|_| "3000".to_string())
                .parse()
                .expect("PORT must be a number"),
            database_url,
            jwt_secret,
            encryption_key,
            default_user_quota: std::env::var("DEFAULT_USER_QUOTA")
                .unwrap_or_else(|_| "0".to_string())
                .parse()
                .unwrap_or(0.0),
            register_enabled: std::env::var("REGISTER_ENABLED")
                .unwrap_or_else(|_| "true".to_string())
                .parse()
                .unwrap_or(true),
            base_url: std::env::var("BASE_URL")
                .unwrap_or_else(|_| "http://localhost:3000".to_string()),
            data_dir,
            portal_dir,
            portal_pro_dir,
            assets_dir,
        }
    }
}
