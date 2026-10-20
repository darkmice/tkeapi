/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia
 * @license        MIT (https://www.tkeapi.com/)
 */

use serde::{Deserialize, Serialize};

/// 站点基本信息设置
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct SiteSettings {
    #[serde(default = "default_site_name")]
    pub name: String,
    #[serde(default = "default_site_name")]
    pub title: String,
    /// 站点简介，用于系统定位与管理后台悬停提示
    #[serde(default)]
    pub intro: String,
    #[serde(default)]
    pub keywords: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub favicon: String,
    #[serde(default)]
    pub logo: String,
    /// 控制台 Logo/站点名点击跳转地址（留空则不可点击）
    #[serde(default)]
    pub logo_title_url: String,
    #[serde(default)]
    pub login_title: String,
    /// 登录页标题点击跳转地址（留空则回退到 logo_title_url，仍空则不可点击）
    #[serde(default)]
    pub login_title_url: String,
    #[serde(default)]
    pub login_subtitle: String,
    #[serde(default = "default_enable_multilingual")]
    pub enable_multilingual: bool,
    /// 站点支持的语言列表（语言代码），如 ["zh", "en"]
    #[serde(default = "default_supported_languages")]
    pub supported_languages: Vec<String>,
    /// 站点默认语言
    #[serde(default = "default_language")]
    pub default_language: String,
    /// 站点运营时区：计费时段、额度刷新、仪表盘/财务自然日等业务切日
    #[serde(default = "default_site_timezone")]
    pub default_timezone: String,
    /// 管理后台展示时区；空则回退站点运营时区
    #[serde(default)]
    pub admin_display_timezone: String,
    /// 是否在前端显示时区后缀
    #[serde(default = "default_show_timezone")]
    pub show_timezone: bool,
    /// 是否允许用户切换亮色/暗色主题（关闭后用户端不显示切换按钮）
    #[serde(default = "default_true_theme")]
    pub enable_theme_toggle: bool,
    /// 站点默认主题："dark" 或 "light"
    #[serde(default = "default_theme_mode")]
    pub default_theme: String,
    /// 版权信息，显示在登录页面底部
    #[serde(default = "default_copyright")]
    pub copyright: String,
    /// 管理后台访问路径，默认 admin1688
    #[serde(default = "default_admin_path")]
    pub admin_path: String,
    /// 登录页风格："split"（左右风格）或 "classic"（经典风格）
    #[serde(default = "default_login_style")]
    pub login_style: String,
    /// 左右风格下的左侧广告语名言
    #[serde(default)]
    pub login_quote: String,
    /// 是否开启注册 IP 黑名单拦截
    #[serde(default)]
    pub ip_blacklist_enabled: bool,
    /// 注册 IP 黑名单列表 (支持单 IP 及 CIDR 网段)
    #[serde(default)]
    pub ip_blacklist: Vec<String>,
    /// 是否开启注册 IP 白名单。命中后不受黑名单与当日注册次数限制
    #[serde(default = "default_true")]
    pub ip_whitelist_enabled: bool,
    /// 注册 IP 白名单。默认放行 10.0.0.0/24、192.168.1.0/24、172.16.0.0/24
    #[serde(default = "default_registration_ip_whitelist")]
    pub ip_whitelist: Vec<String>,
}

pub fn default_registration_ip_whitelist() -> Vec<String> {
    vec![
        "10.0.0.0/24".to_string(),
        "192.168.1.0/24".to_string(),
        "172.16.0.0/24".to_string(),
    ]
}

fn default_login_style() -> String {
    "split".to_string()
}

fn default_admin_path() -> String {
    "admin1688".to_string()
}

fn default_copyright() -> String {
    "© 2026 TkeAPI. All rights reserved.".to_string()
}

fn default_site_name() -> String {
    "Tkeapi".to_string()
}

fn default_true_theme() -> bool {
    true
}

fn default_theme_mode() -> String {
    "dark".to_string()
}

fn default_enable_multilingual() -> bool {
    true
}

fn default_supported_languages() -> Vec<String> {
    vec!["zh".to_string(), "en".to_string()]
}

fn default_language() -> String {
    "zh".to_string()
}

fn default_site_timezone() -> String {
    // timedisplay 默认 UTC+8；不可用 iana_time_zone：进程已锁定 TZ=UTC，会把站点运营时区误判成 UTC。
    crate::time_system::DEFAULT_TIMEDISPLAY.to_string()
}

impl SiteSettings {
    /// 管理后台展示时区；未配置或非法时回退站点运营时区。
    pub fn resolved_admin_display_timezone(&self) -> String {
        crate::time_system::resolve_admin_display_name(
            &self.admin_display_timezone,
            &self.default_timezone,
        )
    }
}

fn default_show_timezone() -> bool {
    true
}

pub const DEFAULT_TOS_ZH: &str = r#"<h2>服务条款 (Terms of Service)</h2>
<p>欢迎使用本平台服务。本服务条款（以下简称“本条款”）是由用户（以下简称“您”）与本平台运营方就本平台提供的各项服务所订立的具有法律约束力的协议。在注册、登录或使用本平台服务之前，请您务必认真阅读并充分理解本条款的全部内容。当您使用本平台提供的任何服务时，即表示您已阅读、理解并同意接受本条款的全部约束。</p>

<h3>一、服务内容与范围</h3>
<p>1. 本平台致力于为开发者与企业提供一站式人工智能（AI）接口中继分发、网关调度、API 令牌管理、在线创作（Playground）以及相关配套工具与技术支持服务。</p>
<p>2. 平台支持多模型统一接入与智能路由，包括但不限于大语言模型（LLM）、图像生成、视频生成、语音合成及自定义工作流接口。</p>
<p>3. 平台有权根据业务发展、技术升级或政策变化，对现有服务内容进行调整、升级、暂停或终止，并在合理时间内通过站内公告或系统消息予以通知。</p>

<h3>二、账户注册与安全规范</h3>
<p>1. 您在注册账户时应提供真实、准确、有效的注册信息（如邮箱地址），并对账户信息的真实性负责。</p>
<p>2. 您应妥善保管自己的账户凭据及在平台内创建的所有 API 令牌（API Keys / Access Tokens）。您对利用该账户或 API 令牌所进行的一切调用、操作及产生的所有结果承担完全责任。</p>
<p>3. 若您发现任何未经授权使用账户或 API 令牌的安全漏洞，应立即通知平台管理员并及时在控制台重置或删除相关令牌。</p>

<h3>三、服务使用规范与合规承诺</h3>
<p>1. 您承诺严格遵守所在国家或地区适用的法律法规、行业规范及公共道德准则，不得利用本平台从事任何违法、侵权、有害或危害网络安全的活动。</p>
<p>2. <strong>严禁行为包括但不限于：</strong></p>
<ul>
  <li>输入、生成或传播含有违法违规、暴力恐吓、淫秽色情、虚假谣言、侵犯他人隐私或知识产权的内容；</li>
  <li>对平台基础设施或上游接口实施恶意扫描、逆向工程、DDoS 攻击、高频恶意刷量等破坏系统稳定性的行为；</li>
  <li>绕过平台的额度限制、并发限制或安全风控策略；</li>
  <li>转售、滥用未经授权的系统资源或从事侵犯第三方合法权益的行为。</li>
</ul>
<p>3. 如用户违反上述规定，平台有权不经事先通知即采取限制调用速率、冻结 API 令牌、扣减违规所得、暂停或永久封禁账户等措施，并保留依法追究法律责任的权利。</p>

<h3>四、计费、充值与消费规则</h3>
<p>1. 本平台服务采用基于实际调用量（如 Token 数、请求次数、计算时长等）的计费模式，具体费率以平台各模型和服务的实时定价规则为准。</p>
<p>2. 您可通过平台支持的在线充值方式或兑换码（Redemption Code）为账户充值。充值金额将计入您的账户可用余额。</p>
<p>3. 除法律法规另有强制性规定或平台明确约定的特殊情形外，账户充值与兑换码一旦核销成功即视为消费履行，原则上不支持退款、提现或转让。</p>

<h3>五、免责声明与服务限制</h3>
<p>1. <strong>第三方模型输出免责：</strong> 本平台作为技术中继与调度网关，所接入的各 AI 模型由独立第三方提供商运行。平台不对任何模型生成内容的真实性、准确性、完整性、合法性或特定用途适用性作任何明示或暗示的保证。生成内容仅供参考，不代表平台立场。</p>
<p>2. <strong>服务可用性保障：</strong> 平台将尽合理商业努力保障系统的高可用性与稳定性。但对于因上游服务商接口波动、网络故障、设备维护、不可抗力等非平台可控原因造成的服务中断、延迟或数据异常，平台不承担违约及赔偿责任。</p>

<h3>六、协议修改与解释权</h3>
<p>平台保留随时修改本服务条款的权利。修改后的条款一旦公布即生效。若您在条款修改后继续使用本服务，即视为您已接受修改后的条款。本条款的最终解释权归平台所有。</p>"#;

pub const DEFAULT_PRIVACY_ZH: &str = r#"<h2>隐私协议 (Privacy Policy)</h2>
<p>本平台非常重视用户的个人信息与隐私安全。本隐私协议（以下简称“本协议”）详细阐述了在您注册、登录、调用 API 及使用本平台各项功能时，我们如何收集、使用、存储、保护及处理您的个人信息和数据。请您在使用我们的服务前仔细阅读并确认已充分理解本协议。</p>

<h3>一、我们收集的信息</h3>
<p>为了向您提供稳定、安全的人工智能 API 中继与管理服务，我们仅收集实现业务功能所必需的最少信息：</p>
<ul>
  <li><strong>账户与身份信息：</strong> 注册及登录时提供的用户名、电子邮箱地址、认证凭据或经您授权的第三方登录信息（如 GitHub、Google、微信等授权标识）。</li>
  <li><strong>API 令牌与配置：</strong> 您在控制台创建的 API 令牌（API Keys）、调用权限配置、自定义模型分组、IP 白名单及限流策略。</li>
  <li><strong>调用与日志数据：</strong> 为保障服务调用链路追踪、计费结算及安全审计，系统会自动记录 API 请求时间戳、调用的模型名称、请求与响应消耗的 Token 数量/额度、客户端 IP 地址、HTTP 状态码及响应耗时。</li>
  <li><strong>交易与资产信息：</strong> 账户余额充值记录、订单流水、兑换码核销记录、消费明细以及您在创作中心或素材库中主动上传和保存的资源。</li>
</ul>

<h3>二、信息的使用目的</h3>
<p>我们收集的信息将严格用于以下用途：</p>
<ol>
  <li><strong>提供与执行服务：</strong> 完成 API 请求的身份鉴权、实时中继转发、响应流推送及创作工具交互；</li>
  <li><strong>计费与账单核算：</strong> 准确计算每次调用的 Token 消耗与费用扣除，维护账户余额真实准确；</li>
  <li><strong>安全防护与防滥用：</strong> 识别异常高频调用、恶意刷量、未授权访问等安全威胁，执行速率限制与风控策略；</li>
  <li><strong>系统运维与优化：</strong> 分析接口调用性能、可用性指标，排查系统故障并改进产品体验；</li>
  <li><strong>客户服务与合规：</strong> 处理您的技术支持工单，并在符合法律法规要求的前提下配合监管部门的必要安全审计。</li>
</ol>

<h3>三、Prompt 与生成内容的数据隐私</h3>
<p>1. 您通过 API 接口或创作中心（Playground）提交的 Prompt（提示词）、上下文文本、图片或音频数据，仅用于实时转发给您所选定的上游 AI 模型供应商进行推理处理。</p>
<p>2. 本平台<strong>不会</strong>将您的提示词和业务内容出售给任何第三方，亦不会将您的私有业务数据用于未经许可的模型训练或商业挖掘。</p>
<p>3. 系统仅在必要范围内对请求进行安全合规过滤或为调试排错提供短期技术日志，日志在保存周期结束后将自动清理。</p>

<h3>四、信息的共享、转让与披露</h3>
<p>1. <strong>必要服务共享：</strong> 为完成您发起的 AI 调用请求，您的请求数据将传输给对应的上游第三方模型提供商（如 OpenAI、Anthropic、Google、智谱、阿里云等）。该等提供商将依据其各自的隐私政策和数据保护协议处理请求。</p>
<p>2. 除上述必要服务履行、获得您的明确同意，或根据法律法规、司法裁决、政府监管机构的法定强制要求外，我们不会向任何无关第三方共享、出售或披露您的个人信息。</p>

<h3>五、数据安全与保护措施</h3>
<p>1. 我们采用行业标准的安全技术措施（如 HTTPS/TLS 全链路传输加密、数据库敏感字段哈希存储、多层防火墙与访问隔离机制）保护您的数据免受未经授权的访问、泄露、篡改或损毁。</p>
<p>2. 用户的密码均经过强加密哈希算法（如 bcrypt/argon2）存储，系统管理员亦无法明文查看您的密码。</p>

<h3>六、您的权利与联系我们</h3>
<p>1. 您有权随时在管理面板中查看、修改您的个人资料，随时生成、禁用或删除您的 API 令牌，以及导出调用明细与账单记录。</p>
<p>2. 如果您对本隐私协议有任何疑问、意见或希望申请注销账户及删除关联数据，请通过平台官方工单系统或管理员联络渠道与我们取得联系。</p>"#;

pub const DEFAULT_TOS_EN: &str = r#"<h2>Terms of Service</h2>
<p>Welcome to our platform. These Terms of Service ("Terms") constitute a legally binding agreement between you ("User", "you") and the operator of this platform ("we", "us", "our"). By registering, accessing, calling our APIs, or otherwise using any services provided by this platform, you acknowledge that you have read, understood, and agreed to be bound by all terms and conditions set forth herein.</p>

<h3>1. Services Provided</h3>
<p>1.1. Our platform provides unified Artificial Intelligence (AI) API relay, intelligent dispatching, API token management, online creative playground, asset management, and related technical infrastructure services.</p>
<p>1.2. The platform aggregates multiple upstream AI models, including Large Language Models (LLMs), image generation, video generation, voice synthesis, and custom workflow relays.</p>
<p>1.3. We reserve the right to modify, upgrade, suspend, or discontinue any feature of the services at our discretion, with reasonable prior notice provided through the platform where feasible.</p>

<h3>2. Account Registration and Security</h3>
<p>2.1. You must provide accurate, current, and complete information during registration and maintain the confidentiality of your account credentials.</p>
<p>2.2. You are solely responsible for all actions, API invocations, and billing incurred under your account and associated API Keys (Access Tokens), whether authorized by you or not.</p>
<p>2.3. If you suspect any compromise, leakage, or unauthorized use of your credentials or API keys, you must immediately revoke or reset the affected keys via the dashboard and notify platform administrators.</p>

<h3>3. Acceptable Use Policy</h3>
<p>3.1. You agree to comply with all applicable local, national, and international laws, regulations, and industry standards when using our services.</p>
<p>3.2. <strong>Prohibited activities include, but are not limited to:</strong></p>
<ul>
  <li>Submitting, generating, or distributing unlawful, violent, sexually explicit, defamatory, fraudulent, or infringing content;</li>
  <li>Conducting cyber attacks, unauthorized vulnerability scanning, reverse engineering, DDoS attacks, or excessive abusive scraping against our infrastructure or upstream providers;</li>
  <li>Circumventing platform rate limits, quota controls, or security monitoring mechanisms;</li>
  <li>Reselling, sublicensing, or exploiting platform resources without explicit written authorization.</li>
</ul>
<p>3.3. Any breach of this policy may result in immediate rate throttling, API key suspension, balance forfeiture, or permanent account termination without prior notice, along with potential legal action.</p>

<h3>4. Billing, Pricing and Payments</h3>
<p>4.1. Usage is billed based on real-time consumption (e.g., token count, request volume, or compute duration) in accordance with the current pricing schedule published on the platform.</p>
<p>4.2. You may add funds to your account balance via supported payment methods or redeem authorized coupon/redemption codes.</p>
<p>4.3. All purchases, balance top-ups, and coupon redemptions are final and non-refundable once credited, except where mandatory applicable law provides otherwise.</p>

<h3>5. Disclaimers and Limitation of Liability</h3>
<p>5.1. <strong>Third-Party Model Outputs:</strong> The platform acts as a technology gateway and proxy. AI model outputs are generated dynamically by third-party upstream providers. We make no warranties, express or implied, regarding the accuracy, completeness, validity, or fitness for a particular purpose of any generated content.</p>
<p>5.2. <strong>Service Availability:</strong> While we strive for high availability and low latency, we do not guarantee uninterrupted or error-free service. We are not liable for upstream service disruptions, latency spikes, force majeure events, or downstream indirect losses.</p>

<h3>6. Amendments and Governing Law</h3>
<p>We reserve the right to revise these Terms at any time. Continued use of the platform following the posting of updated Terms constitutes your binding acceptance of the revised Terms.</p>"#;

pub const DEFAULT_PRIVACY_EN: &str = r#"<h2>Privacy Policy</h2>
<p>We are committed to safeguarding your privacy and personal data. This Privacy Policy explains how we collect, use, process, disclose, and protect your information when you access our platform, register an account, utilize our APIs, or interact with our services. Please review this policy carefully.</p>

<h3>1. Information We Collect</h3>
<p>We collect only the minimum necessary data required to deliver reliable and secure AI API relay services:</p>
<ul>
  <li><strong>Account & Profile Data:</strong> Username, email address, authentication credentials, or authorization identifiers from third-party OAuth providers (e.g., GitHub, Google, WeChat).</li>
  <li><strong>API Keys & Configurations:</strong> API tokens created in your dashboard, permission scopes, model routing rules, IP whitelists, and rate limit settings.</li>
  <li><strong>Usage & Operational Logs:</strong> Request timestamps, requested model names, token consumption metrics, client IP addresses, HTTP status codes, and request durations for billing and auditing purposes.</li>
  <li><strong>Billing & Assets:</strong> Top-up order history, balance ledgers, redemption code records, and files or assets uploaded in your creative workspace.</li>
</ul>

<h3>2. How We Use Your Information</h3>
<p>Your information is used strictly for the following purposes:</p>
<ol>
  <li><strong>Service Delivery:</strong> Authenticating API requests, performing real-time request routing, streaming responses, and managing platform features;</li>
  <li><strong>Billing & Account Management:</strong> Calculating real-time token and credit consumption to maintain accurate ledger balances;</li>
  <li><strong>Security & Abuse Prevention:</strong> Detecting fraudulent calls, enforcing rate limits, mitigating malicious traffic, and ensuring platform integrity;</li>
  <li><strong>Infrastructure Optimization:</strong> Monitoring system reliability, debugging performance bottlenecks, and improving user experience;</li>
  <li><strong>Compliance & Support:</strong> Responding to support inquiries and complying with statutory audit and legal retention requirements.</li>
</ol>

<h3>3. Data Privacy of Prompts & Generated Content</h3>
<p>3.1. Prompts, messages, images, and other inputs submitted through our APIs or playground are transmitted in real time to the selected upstream AI model providers solely for inference execution.</p>
<p>3.2. We do <strong>NOT</strong> sell your prompts or proprietary business data to third parties, nor do we use your private data for unconsented model training.</p>
<p>3.3. Technical logs are retained only for the duration necessary for audit, debugging, and billing validation, after which they are systematically purged.</p>

<h3>4. Data Sharing & Third-Party Providers</h3>
<p>4.1. <strong>Upstream AI Providers:</strong> In order to fulfill your API requests, necessary payload data is forwarded to the designated upstream providers (such as OpenAI, Anthropic, Google, Zhipu, Alibaba Cloud, etc.), who process data in accordance with their respective privacy policies.</p>
<p>4.2. We do not sell, rent, or trade your personal data to third parties. We disclose data only when legally required by judicial or regulatory authorities.</p>

<h3>5. Data Security Measures</h3>
<p>We implement robust technical and organizational security measures, including end-to-end TLS/HTTPS encryption, strong cryptographic password hashing (e.g., bcrypt/argon2), and strict role-based access controls to prevent unauthorized access, alteration, or disclosure of your data.</p>

<h3>6. Your Rights and Contact</h3>
<p>You have the right to access, update, or revoke your API keys and profile information at any time via the console. If you have questions regarding this Privacy Policy or wish to request account deletion, please contact platform support.</p>"#;

/// 站点协议设置
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AgreementSettings {
    #[serde(default = "default_agreement_mode")]
    pub tos_mode: String, // "text" or "link"
    #[serde(default = "default_agreement_mode")]
    pub tos_mode_en: String,
    #[serde(default = "default_tos_content_zh")]
    pub tos_content: String,
    #[serde(default = "default_tos_content_en")]
    pub tos_content_en: String,
    #[serde(default)]
    pub tos_link: String,
    #[serde(default)]
    pub tos_link_en: String,
    #[serde(default = "default_agreement_mode")]
    pub privacy_mode: String, // "text" or "link"
    #[serde(default = "default_agreement_mode")]
    pub privacy_mode_en: String,
    #[serde(default = "default_privacy_content_zh")]
    pub privacy_content: String,
    #[serde(default = "default_privacy_content_en")]
    pub privacy_content_en: String,
    #[serde(default)]
    pub privacy_link: String,
    #[serde(default)]
    pub privacy_link_en: String,
    #[serde(default = "default_true")]
    pub tos_enabled: bool,
    #[serde(default = "default_true")]
    pub privacy_enabled: bool,
}

impl Default for AgreementSettings {
    fn default() -> Self {
        Self {
            tos_mode: default_agreement_mode(),
            tos_mode_en: default_agreement_mode(),
            tos_content: default_tos_content_zh(),
            tos_content_en: default_tos_content_en(),
            tos_link: String::new(),
            tos_link_en: String::new(),
            privacy_mode: default_agreement_mode(),
            privacy_mode_en: default_agreement_mode(),
            privacy_content: default_privacy_content_zh(),
            privacy_content_en: default_privacy_content_en(),
            privacy_link: String::new(),
            privacy_link_en: String::new(),
            tos_enabled: true,
            privacy_enabled: true,
        }
    }
}

fn default_agreement_mode() -> String {
    "text".to_string()
}

fn default_tos_content_zh() -> String {
    DEFAULT_TOS_ZH.to_string()
}

fn default_tos_content_en() -> String {
    DEFAULT_TOS_EN.to_string()
}

fn default_privacy_content_zh() -> String {
    DEFAULT_PRIVACY_ZH.to_string()
}

fn default_privacy_content_en() -> String {
    DEFAULT_PRIVACY_EN.to_string()
}

/// 辅助货币设置
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AuxiliaryCurrency {
    #[serde(default)]
    pub code: String, // e.g., "USD"
    #[serde(default)]
    pub symbol: String, // e.g., "$"
    #[serde(default)]
    pub exchange_rate: f64, // e.g., if default is CNY and this is USD, rate could be 0.14
    #[serde(default = "default_true")]
    pub enabled: bool,
}

/// 单币种汇率配置。汇率口径：1 默认货币 = 该币种的汇率
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct FxCurrencyConfig {
    #[serde(default)]
    pub code: String,
    /// 单币种手动：开启后该币种固定使用 manual_rate，不跟随自动汇率。
    /// `adjust = "manual"` 时与此同时为 true；旧数据只有该字段。
    #[serde(default)]
    pub manual: bool,
    #[serde(default)]
    pub manual_rate: f64,
    /// 空：跟随实时汇率（旧的整站 `markup_percent` 仍加在收款汇率上）。
    /// `percent`：实时汇率 × adjust_value%。100 为在线汇率，110 为 1.1 倍，50 为一半。
    /// `amount`：实时汇率 + adjust_value。
    /// `manual`：使用 manual_rate，不再叠加整站加价。
    #[serde(default)]
    pub adjust: String,
    #[serde(default)]
    pub adjust_value: f64,
    /// 是否作为辅助货币在模型广场、模型列表等页面展示
    #[serde(default = "default_true")]
    pub display: bool,
}

/// 汇率设置：auto 按免费汇率源定时刷新；manual 全部使用手动汇率。
/// `live_enabled` 默认关闭：国内单币种不请求外部汇率，只按默认货币收款。
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct FxSettings {
    /// 实时汇率总开关。关闭后不拉取汇率，跨币种渠道不可收款。
    #[serde(default)]
    pub live_enabled: bool,
    #[serde(default = "default_fx_mode")]
    pub mode: String,
    #[serde(default = "default_fx_refresh_interval_minutes")]
    pub refresh_interval_minutes: u32,
    /// 旧版整站加价。币种未单独配置 `adjust` 时仍加在收款汇率上；
    /// 新配置把加价写进各币种后此处为 0。
    #[serde(default)]
    pub markup_percent: f64,
    /// 自动汇率超过该小时数未更新时停止跨币种收款
    #[serde(default = "default_fx_max_stale_hours")]
    pub max_stale_hours: u32,
    /// 手动汇率保存时的默认货币；与当前默认货币不一致时手动汇率作废
    #[serde(default)]
    pub manual_base: String,
    #[serde(default)]
    pub currencies: Vec<FxCurrencyConfig>,
}

fn default_fx_mode() -> String {
    "auto".to_string()
}

fn default_fx_refresh_interval_minutes() -> u32 {
    60
}

fn default_fx_max_stale_hours() -> u32 {
    48
}

impl Default for FxSettings {
    fn default() -> Self {
        Self {
            live_enabled: false,
            mode: default_fx_mode(),
            refresh_interval_minutes: default_fx_refresh_interval_minutes(),
            markup_percent: 0.0,
            max_stale_hours: default_fx_max_stale_hours(),
            manual_base: String::new(),
            currencies: Vec::new(),
        }
    }
}

/// 货币设置
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct CurrencySettings {
    #[serde(default)]
    pub default_currency: String,
    #[serde(default)]
    pub currency_symbol: String,
    #[serde(default)]
    pub currency_unit: String,
    #[serde(default)]
    pub token_ratio: f64,
    /// 对外由 `fx` 与实时汇率推导；库内仅残留旧版手填值，读取时作为汇率初始值
    #[serde(default)]
    pub auxiliary_currencies: Vec<AuxiliaryCurrency>,
    #[serde(default)]
    pub fx: FxSettings,
    #[serde(default = "default_quick_amounts")]
    pub quick_amounts: Vec<f64>,
    #[serde(default = "default_min_recharge_amount")]
    pub min_recharge_amount: f64,
    #[serde(default = "default_max_recharge_amount")]
    pub max_recharge_amount: f64,
}

fn default_quick_amounts() -> Vec<f64> {
    vec![20.0, 50.0, 100.0, 500.0, 1000.0, 5000.0]
}

fn default_min_recharge_amount() -> f64 {
    5.0
}

fn default_max_recharge_amount() -> f64 {
    10000.0
}

/// 登录方式设置 — 控制用户端可用的登录方式
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct LoginSettings {
    #[serde(default = "default_true")]
    pub enable_username_login: bool,
    #[serde(default)]
    pub enable_mobile_login: bool,
    #[serde(default)]
    pub enable_email_login: bool,
    #[serde(default)]
    pub enable_wechat_login: bool,
    #[serde(default)]
    pub enable_google_login: bool,
}

/// 注册方式设置 — 控制用户端可用的注册方式及安全策略
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct RegistrationSettings {
    #[serde(default)]
    pub enable_username_registration: bool,
    /// 用户名直接注册的独立 IP 每日限制（默认 3；清空为 None 或 <=0 时遵循站点安全 IP 防刷规则）
    #[serde(default = "default_username_ip_daily_limit")]
    pub username_ip_daily_limit: Option<i64>,
    #[serde(default)]
    pub enable_email_registration: bool,
    #[serde(default)]
    pub enable_mobile_registration: bool,
    #[serde(default)]
    pub enable_password_recovery: bool,
    /// IP 防刷：开启后限制同 IP 每日注册次数
    #[serde(default)]
    pub ip_rate_limit_enabled: bool,
    /// 同 IP 每日最多注册数
    #[serde(default = "default_ip_daily_limit")]
    pub ip_daily_limit: i64,
    /// 邮箱防刷：开启后 @ 前仅允许数字+字母+"_"，长度≤25
    #[serde(default)]
    pub email_validation_strict: bool,
    /// 邮箱白名单：开启后仅允许指定域名邮箱注册
    #[serde(default)]
    pub email_whitelist_enabled: bool,
    /// 允许的邮箱域名列表
    #[serde(default = "default_email_whitelist")]
    pub email_whitelist: Vec<String>,
    /// 是否要求绑定手机号（纳入绑定策略）
    #[serde(default)]
    pub require_bind_mobile: bool,
    /// 是否要求绑定邮箱（纳入绑定策略）
    #[serde(default)]
    pub require_bind_email: bool,
    /// 绑定执行方式：all=弹窗必须绑定全部 / any=弹窗必须绑定其一 / prompt_all=仅弹窗提示绑定全部 / prompt_any=仅弹窗提示绑定其一
    /// prompt_only 为旧值，等同 prompt_all
    #[serde(default = "default_bind_enforcement")]
    pub bind_enforcement: String,
    /// 是否开启站点用户实名认证（KYC）
    #[serde(default)]
    pub enable_user_kyc: bool,
    /// 同一真实邮箱最多绑定的用户数，默认 5
    #[serde(default = "default_max_accounts_per_contact")]
    pub max_accounts_per_email: i64,
    /// 同一手机号最多绑定的用户数，默认 5
    #[serde(default = "default_max_accounts_per_contact")]
    pub max_accounts_per_mobile: i64,
    /// 同一微信最多绑定的用户数，默认 5
    #[serde(default = "default_max_accounts_per_contact")]
    pub max_accounts_per_wechat: i64,
    /// 允许国际手机号（E.164）收短信验证码；关闭时只接受中国大陆 11 位号码
    #[serde(default)]
    pub allow_international_mobile: bool,
}

fn default_username_ip_daily_limit() -> Option<i64> {
    Some(3)
}

fn default_bind_enforcement() -> String {
    "all".to_string()
}

fn default_max_accounts_per_contact() -> i64 {
    5
}

impl RegistrationSettings {
    /// 是否启用了任一绑定通道
    pub fn bind_policy_active(&self) -> bool {
        self.require_bind_mobile || self.require_bind_email
    }

    pub fn has_valid_mobile(mobile: Option<&str>) -> bool {
        mobile.map(|m| !m.trim().is_empty()).unwrap_or(false)
    }

    pub fn has_valid_email(email: &str) -> bool {
        let email = email.trim();
        !email.is_empty() && !email.ends_with("@tokensbyte.local")
    }

    /// 只弹窗，不拦截创建令牌。prompt_only 为旧值，等同 prompt_all
    pub fn is_prompt_enforcement(&self) -> bool {
        matches!(
            self.bind_enforcement.as_str(),
            "prompt_only" | "prompt_all" | "prompt_any"
        )
    }

    /// 勾选项目里绑定一项即满足
    fn requires_any_bind(&self) -> bool {
        matches!(self.bind_enforcement.as_str(), "any" | "prompt_any")
    }

    /// 用户当前绑定是否满足策略（弹窗是否关闭、硬拦截是否放行都用此判断）
    pub fn is_bind_satisfied(&self, email: &str, mobile: Option<&str>) -> bool {
        if !self.bind_policy_active() {
            return true;
        }
        let has_mobile = Self::has_valid_mobile(mobile);
        let has_email = Self::has_valid_email(email);
        match (self.require_bind_mobile, self.require_bind_email) {
            (true, true) => {
                if self.requires_any_bind() {
                    has_mobile || has_email
                } else {
                    has_mobile && has_email
                }
            }
            (true, false) => has_mobile,
            (false, true) => has_email,
            (false, false) => true,
        }
    }

    /// 创建令牌等敏感操作是否应硬拦截
    pub fn should_block_token_create(&self, email: &str, mobile: Option<&str>) -> bool {
        self.bind_policy_active()
            && !self.is_prompt_enforcement()
            && !self.is_bind_satisfied(email, mobile)
    }

    /// 生成创建令牌失败时的提示文案
    pub fn token_bind_block_message(&self) -> String {
        match (self.require_bind_mobile, self.require_bind_email) {
            (true, true) if self.requires_any_bind() => {
                "创建 API 令牌前请先绑定手机号或邮箱".to_string()
            }
            (true, true) => "创建 API 令牌前请先绑定手机号和邮箱".to_string(),
            (true, false) => "创建 API 令牌前请先绑定手机号".to_string(),
            (false, true) => "创建 API 令牌前请先绑定邮箱".to_string(),
            _ => "创建 API 令牌前请先完成账号绑定".to_string(),
        }
    }
}

fn default_true() -> bool {
    true
}

fn default_ip_daily_limit() -> i64 {
    6
}

fn default_email_whitelist() -> Vec<String> {
    vec![
        "qq.com".to_string(),
        "163.com".to_string(),
        "outlook.com".to_string(),
        "aliyun.com".to_string(),
        "foxmail.com".to_string(),
    ]
}

/// SMTP 邮箱通知设置
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct SMTPSettings {
    #[serde(default)]
    pub host: String,
    #[serde(default)]
    pub port: u16,
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub password: String,
    #[serde(default)]
    pub from_address: String,
    #[serde(default)]
    pub from_name: String,
}

/// 短信通知设置（provider: tencent | volcengine；缺省 tencent 兼容旧配置）
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SmsSettings {
    /// 服务商：tencent（腾讯云）| volcengine（火山引擎）
    #[serde(default = "default_sms_provider")]
    pub provider: String,
    /// 腾讯云 SecretId / 火山 Access Key
    #[serde(default)]
    pub secret_id: String,
    /// 腾讯云 SecretKey / 火山 Secret Key
    #[serde(default)]
    pub secret_key: String,
    /// 腾讯云 SdkAppId / 火山消息组 ID（SmsAccount）
    #[serde(default)]
    pub sdk_app_id: String,
    /// 已审核的短信签名
    #[serde(default)]
    pub sign_name: String,
    /// 验证码模板 ID
    #[serde(default)]
    pub template_id: String,
    /// 余额不足提醒模板 ID（无变量固定正文）
    #[serde(default)]
    pub balance_template_id: String,
    /// 火山验证码模板变量名（TemplateParam JSON 键，默认 code）
    #[serde(default = "default_sms_code_param")]
    pub code_param: String,
}

fn default_sms_provider() -> String {
    "tencent".to_string()
}

fn default_sms_code_param() -> String {
    "code".to_string()
}

impl Default for SmsSettings {
    fn default() -> Self {
        Self {
            provider: default_sms_provider(),
            secret_id: String::new(),
            secret_key: String::new(),
            sdk_app_id: String::new(),
            sign_name: String::new(),
            template_id: String::new(),
            balance_template_id: String::new(),
            code_param: default_sms_code_param(),
        }
    }
}

impl SmsSettings {
    pub fn is_volcengine(&self) -> bool {
        self.provider.trim().eq_ignore_ascii_case("volcengine")
    }

    pub fn credentials_configured(&self) -> bool {
        !self.secret_id.trim().is_empty() && !self.secret_key.trim().is_empty()
    }

    /// 是否已配置余额提醒模板（开启短信余额提醒前的前置条件）
    pub fn balance_template_configured(&self) -> bool {
        !self.balance_template_id.trim().is_empty()
    }

    /// 发送时使用的余额模板 ID（去空白）
    pub fn balance_template_id_effective(&self) -> &str {
        self.balance_template_id.trim()
    }

    /// 火山验证码变量名（与控制台变量名一致，如 1 / code，勿带 ${}）
    pub fn code_param_effective(&self) -> &str {
        let p = self.code_param.trim();
        if p.is_empty() {
            "code"
        } else {
            p
        }
    }
}

/// 营销设置
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct MarketingSettings {
    #[serde(default)]
    pub enable_registration_gift: bool,
    /// 是否开启用户端兑换码功能
    #[serde(default)]
    pub enable_redemption: bool,
    #[serde(default = "default_gift_mode")]
    pub gift_mode: String, // "fixed" or "random"
    #[serde(default)]
    pub fixed_amount: f64,
    #[serde(default)]
    pub min_amount: f64,
    #[serde(default)]
    pub max_amount: f64,
}

fn default_gift_mode() -> String {
    "fixed".to_string()
}

/// 数据库连接设置
#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq, Eq)]
pub struct DatabaseSettings {
    #[serde(default)]
    pub db_type: String, // "postgres"
    #[serde(default)]
    pub host: String,
    #[serde(default)]
    pub port: u16,
    #[serde(default)]
    pub database: String,
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub password: String,
    #[serde(default)]
    pub ssl_mode: bool,
}

/// 微信支付设置
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct PaymentWechatSettings {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub mchid: String,
    #[serde(default)]
    pub appid: String,
    #[serde(default)]
    pub api_v3_key: String,
    #[serde(default)]
    pub cert_serial_no: String,
    #[serde(default)]
    pub private_key: String,
}

/// 支付宝设置
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct PaymentAlipaySettings {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub app_id: String,
    #[serde(default)]
    pub private_key: String,
    #[serde(default)]
    pub alipay_public_key: String,
    #[serde(default = "default_sign_type")]
    pub sign_type: String,
}

fn default_sign_type() -> String {
    "RSA2".to_string()
}

/// Stripe 支付设置
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct PaymentStripeSettings {
    #[serde(default)]
    pub enabled: bool,
    /// Stripe Secret Key (sk_live_xxx 或 sk_test_xxx)
    #[serde(default)]
    pub secret_key: String,
    /// Stripe Publishable Key (pk_live_xxx 或 pk_test_xxx)
    #[serde(default)]
    pub publishable_key: String,
    /// Stripe Webhook Signing Secret (whsec_xxx)
    #[serde(default)]
    pub webhook_secret: String,
}

fn default_crypto_exchange_rate() -> f64 {
    1.0
}

/// HyperBC 加密货币支付设置
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct PaymentHyperbcSettings {
    #[serde(default)]
    pub enabled: bool,
    /// CipherBC 分配的 APP_ID
    #[serde(default)]
    pub app_id: String,
    /// 商户 RSA 私钥 (PEM 格式，用于请求签名)
    #[serde(default)]
    pub merchant_private_key: String,
    /// CipherBC 平台 RSA 公钥 (PEM 格式，用于验证回调签名)
    #[serde(default)]
    pub hyperbc_public_key: String,
    /// API 接口地址
    #[serde(default = "default_hyperbc_api_url")]
    pub api_url: String,
    /// USDT/加密货币 兑换系统货币的汇率
    #[serde(default = "default_crypto_exchange_rate")]
    pub crypto_exchange_rate: f64,
}

fn default_hyperbc_api_url() -> String {
    "https://api.hyperbc.com".to_string()
}

/// 通联支付设置
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct PaymentAllinpaySettings {
    /// 是否启用通联支付方式
    #[serde(default)]
    pub enabled: bool,
    /// 实际交易商户号 (cusid)
    #[serde(default)]
    pub cusid: String,
    /// 平台分配的应用ID (appid)
    #[serde(default)]
    pub appid: String,
    /// 商户 RSA 私钥 (PKCS#1 Base64/PEM，对应通联「RSA公钥」栏位上传的商户公钥)
    #[serde(default)]
    pub merchant_private_key: String,
    /// 通联平台 RSA 公钥 (PEM，填商服「通联RSA公钥」，用于回调/查询验签)
    #[serde(default)]
    pub allinpay_public_key: String,
    /// 签名类型固定为 RSA（SHA1WithRSA）；保留字段以兼容已存配置
    #[serde(default = "default_allinpay_sign_type")]
    pub sign_type: String,
    /// 接口网关地址
    #[serde(default = "default_allinpay_api_url")]
    pub api_url: String,
    /// 统一支付业务接口协议版本
    #[serde(default = "default_allinpay_version")]
    pub version: String,
}

fn default_allinpay_api_url() -> String {
    "https://vsp.allinpay.com/apiweb".to_string()
}

fn default_allinpay_version() -> String {
    "11".to_string()
}

fn default_allinpay_sign_type() -> String {
    "RSA".to_string()
}

/// 支付渠道用户端展示配置（与各 payment_* 密钥配置解耦）
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct PaymentChannelsUiSettings {
    #[serde(default)]
    pub channels: Vec<PaymentChannelUiItem>,
}

/// 单个支付渠道的展示与排序
///
/// - 普通渠道 id 即用户端 payment_method（如 alipay / wechat）
/// - 通联聚合渠道 id 为 `allinpay`；实际下单仍用 `allinpay_wechat` / `allinpay_alipay`
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct PaymentChannelUiItem {
    #[serde(default)]
    pub id: String,
    /// 排序权重，数字越大越靠前
    #[serde(default)]
    pub sort_order: i64,
    #[serde(default)]
    pub enabled: bool,
    /// 用户端中文显示名称；空则使用系统默认
    #[serde(default)]
    pub display_name: Option<String>,
    /// 用户端英文显示名称；空则使用系统默认。非中文站点语言使用此字段
    #[serde(default)]
    pub display_name_en: Option<String>,
    /// 用户端中文副标题/角标；空则使用系统默认
    #[serde(default)]
    pub subtitle: Option<String>,
    /// 用户端英文副标题/角标；空则使用系统默认
    #[serde(default)]
    pub subtitle_en: Option<String>,
    /// Logo 图片 URL；空则使用系统默认图标
    #[serde(default)]
    pub logo_url: Option<String>,
    /// 通联子渠道：微信（仅 id=allinpay 有效）
    #[serde(default = "default_true")]
    pub allinpay_wechat_enabled: bool,
    /// 通联子渠道：支付宝（仅 id=allinpay 有效）
    #[serde(default = "default_true")]
    pub allinpay_alipay_enabled: bool,
    /// 渠道收款币种（按顺序优先）；空则使用渠道默认能力，见 `services::fx_rates`
    #[serde(default)]
    pub currencies: Option<Vec<String>>,
    /// 开启后，该渠道的钱包充值使用下面的最小/最大金额，不再使用全局通用充值设置
    #[serde(default)]
    pub recharge_limit_enabled: bool,
    /// 渠道最小充值（站点默认货币）。仅 `recharge_limit_enabled` 时生效；0 表示不限制
    #[serde(default)]
    pub min_recharge_amount: f64,
    /// 渠道最大充值（站点默认货币）。仅 `recharge_limit_enabled` 时生效；0 表示不限制
    #[serde(default)]
    pub max_recharge_amount: f64,
}

/// 公开支付渠道（不含密钥）
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct PublicPaymentChannel {
    pub id: String,
    pub sort_order: i64,
    pub enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name_en: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subtitle: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subtitle_en: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logo_url: Option<String>,
    /// 通联已开启的子渠道 payment_method 列表（如 allinpay_wechat）
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allinpay_methods: Vec<String>,
    /// 渠道可收款币种
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub currencies: Vec<String>,
    /// 按站点默认货币下单时，该渠道实际收款币种
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pay_currency: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pay_currency_symbol: Option<String>,
    /// 1 默认货币 = pay_rate 收款币种（跨币种时已含加价；同币种为 1）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pay_rate: Option<f64>,
    /// 收款币种最小单位小数位
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pay_decimals: Option<u32>,
    /// 是否启用渠道单独充值限额；关闭时前端沿用全局最小/最大充值
    #[serde(default)]
    pub recharge_limit_enabled: bool,
    #[serde(default)]
    pub min_recharge_amount: f64,
    #[serde(default)]
    pub max_recharge_amount: f64,
}

/// 系统内置支付渠道 ID 与默认排序（越大越靠前）
pub fn payment_channel_catalog() -> &'static [(&'static str, i64)] {
    &[
        ("alipay", 70),
        ("wechat", 60),
        ("allinpay", 50),
        ("stripe", 30),
        ("hyperbc", 10),
    ]
}

fn trim_opt(s: &Option<String>) -> Option<String> {
    s.as_ref()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

impl PaymentChannelUiItem {
    pub fn normalized_display(&self) -> Option<String> {
        trim_opt(&self.display_name)
    }

    pub fn normalized_display_en(&self) -> Option<String> {
        trim_opt(&self.display_name_en)
    }

    pub fn normalized_subtitle(&self) -> Option<String> {
        trim_opt(&self.subtitle)
    }

    pub fn normalized_subtitle_en(&self) -> Option<String> {
        trim_opt(&self.subtitle_en)
    }

    pub fn normalized_logo_url(&self) -> Option<String> {
        trim_opt(&self.logo_url)
    }

    pub fn allinpay_methods(&self) -> Vec<String> {
        let mut methods = Vec::new();
        if self.allinpay_wechat_enabled {
            methods.push("allinpay_wechat".to_string());
        }
        if self.allinpay_alipay_enabled {
            methods.push("allinpay_alipay".to_string());
        }
        methods
    }
}

/// 用已存配置合并内置渠道目录；兼容旧版拆分的 allinpay_wechat / allinpay_alipay
pub fn merge_payment_channels_ui(
    saved: Option<PaymentChannelsUiSettings>,
    gateway: &PaymentGatewayEnableFlags,
) -> PaymentChannelsUiSettings {
    let mut by_id: std::collections::HashMap<String, PaymentChannelUiItem> = saved
        .unwrap_or_default()
        .channels
        .into_iter()
        .filter(|c| !c.id.trim().is_empty())
        .map(|c| (c.id.clone(), c))
        .collect();

    // 旧数据：两个通联子渠道 → 合并为 allinpay
    let legacy_wechat = by_id.remove("allinpay_wechat");
    let legacy_alipay = by_id.remove("allinpay_alipay");
    if !by_id.contains_key("allinpay") && (legacy_wechat.is_some() || legacy_alipay.is_some()) {
        let lw = legacy_wechat.as_ref();
        let la = legacy_alipay.as_ref();
        let sort_order = lw
            .map(|c| c.sort_order)
            .into_iter()
            .chain(la.map(|c| c.sort_order))
            .max()
            .unwrap_or(50);
        let enabled =
            lw.map(|c| c.enabled).unwrap_or(false) || la.map(|c| c.enabled).unwrap_or(false);
        let display = lw
            .and_then(|c| c.normalized_display())
            .or_else(|| la.and_then(|c| c.normalized_display()));
        let display_en = lw
            .and_then(|c| c.normalized_display_en())
            .or_else(|| la.and_then(|c| c.normalized_display_en()));
        let subtitle = lw
            .and_then(|c| c.normalized_subtitle())
            .or_else(|| la.and_then(|c| c.normalized_subtitle()));
        let subtitle_en = lw
            .and_then(|c| c.normalized_subtitle_en())
            .or_else(|| la.and_then(|c| c.normalized_subtitle_en()));
        let logo = lw
            .and_then(|c| c.normalized_logo_url())
            .or_else(|| la.and_then(|c| c.normalized_logo_url()));
        by_id.insert(
            "allinpay".to_string(),
            PaymentChannelUiItem {
                id: "allinpay".to_string(),
                sort_order,
                enabled,
                display_name: display,
                display_name_en: display_en,
                subtitle,
                subtitle_en,
                logo_url: logo,
                allinpay_wechat_enabled: lw.map(|c| c.enabled).unwrap_or(gateway.allinpay),
                allinpay_alipay_enabled: la.map(|c| c.enabled).unwrap_or(gateway.allinpay),
                currencies: None,
                recharge_limit_enabled: false,
                min_recharge_amount: 0.0,
                max_recharge_amount: 0.0,
            },
        );
    }

    let mut channels = Vec::with_capacity(payment_channel_catalog().len());
    for &(id, default_sort) in payment_channel_catalog() {
        if let Some(mut item) = by_id.remove(id) {
            if item.id.is_empty() {
                item.id = id.to_string();
            }
            if id == "allinpay"
                && !item.allinpay_wechat_enabled
                && !item.allinpay_alipay_enabled
                && item.enabled
            {
                // 开启主开关但未指定子渠道时，默认两边都开
                item.allinpay_wechat_enabled = true;
                item.allinpay_alipay_enabled = true;
            }
            channels.push(item);
        } else {
            let gateway_on = gateway.default_enabled_for(id);
            channels.push(PaymentChannelUiItem {
                id: id.to_string(),
                sort_order: default_sort,
                enabled: gateway_on,
                display_name: None,
                display_name_en: None,
                subtitle: None,
                subtitle_en: None,
                logo_url: None,
                allinpay_wechat_enabled: true,
                allinpay_alipay_enabled: true,
                currencies: None,
                recharge_limit_enabled: false,
                min_recharge_amount: 0.0,
                max_recharge_amount: 0.0,
            });
        }
    }
    PaymentChannelsUiSettings { channels }
}

/// 各网关当前启用状态（用于补齐渠道默认 enabled）
#[derive(Debug, Clone, Default)]
pub struct PaymentGatewayEnableFlags {
    pub wechat: bool,
    pub alipay: bool,
    pub stripe: bool,
    pub hyperbc: bool,
    pub allinpay: bool,
}

impl PaymentGatewayEnableFlags {
    pub fn default_enabled_for(&self, channel_id: &str) -> bool {
        match channel_id {
            "wechat" => self.wechat,
            "alipay" => self.alipay,
            "stripe" => self.stripe,
            "hyperbc" => self.hyperbc,
            "allinpay" => self.allinpay,
            _ => false,
        }
    }

    pub fn gateway_ready_for(&self, channel_id: &str) -> bool {
        self.default_enabled_for(channel_id)
    }
}

/// 计算公开渠道列表：渠道 enabled ∧ 对应网关 enabled
pub fn build_public_payment_channels(
    ui: &PaymentChannelsUiSettings,
    gateway: &PaymentGatewayEnableFlags,
) -> Vec<PublicPaymentChannel> {
    let mut list: Vec<PublicPaymentChannel> = ui
        .channels
        .iter()
        .map(|c| {
            let gateway_on = gateway.gateway_ready_for(&c.id);
            let methods = if c.id == "allinpay" {
                c.allinpay_methods()
            } else {
                Vec::new()
            };
            let channel_on = if c.id == "allinpay" {
                c.enabled && !methods.is_empty()
            } else {
                c.enabled
            };
            PublicPaymentChannel {
                id: c.id.clone(),
                sort_order: c.sort_order,
                enabled: channel_on && gateway_on,
                display_name: c.normalized_display(),
                display_name_en: c.normalized_display_en(),
                subtitle: c.normalized_subtitle(),
                subtitle_en: c.normalized_subtitle_en(),
                logo_url: c.normalized_logo_url(),
                allinpay_methods: if c.id == "allinpay" && gateway_on && c.enabled {
                    methods
                } else {
                    Vec::new()
                },
                recharge_limit_enabled: c.recharge_limit_enabled,
                min_recharge_amount: c.min_recharge_amount,
                max_recharge_amount: c.max_recharge_amount,
                ..Default::default()
            }
        })
        .collect();
    list.sort_by(|a, b| {
        b.sort_order
            .cmp(&a.sort_order)
            .then_with(|| a.id.cmp(&b.id))
    });
    list
}

/// 从公开渠道列表推导兼容旧字段的 PublicPaymentStatus
pub fn public_payment_status_from_channels(
    channels: &[PublicPaymentChannel],
) -> PublicPaymentStatus {
    let on = |id: &str| channels.iter().any(|c| c.id == id && c.enabled);
    PublicPaymentStatus {
        wechat_enabled: on("wechat"),
        alipay_enabled: on("alipay"),
        stripe_enabled: on("stripe"),
        hyperbc_enabled: on("hyperbc"),
        allinpay_enabled: on("allinpay"),
    }
}

/// 查询某下单 payment_method 在 UI 配置中是否允许
///
/// - 普通渠道：对应 id 的 enabled
/// - 通联子渠道：优先读聚合渠道 `allinpay` 的子开关；兼容旧版拆分 id
/// - 配置完全缺失时默认 true（由网关 enabled 兜底，兼容未迁移数据）
pub fn is_payment_channel_ui_enabled(ui: &PaymentChannelsUiSettings, channel_id: &str) -> bool {
    if channel_id == "allinpay_wechat" || channel_id == "allinpay_alipay" {
        if let Some(c) = ui.channels.iter().find(|c| c.id == "allinpay") {
            return c.enabled
                && if channel_id == "allinpay_wechat" {
                    c.allinpay_wechat_enabled
                } else {
                    c.allinpay_alipay_enabled
                };
        }
        // 兼容尚未迁移的拆分配置
        if let Some(c) = ui.channels.iter().find(|c| c.id == channel_id) {
            return c.enabled;
        }
        return true;
    }

    ui.channels
        .iter()
        .find(|c| c.id == channel_id)
        .map(|c| c.enabled)
        .unwrap_or(true)
}

fn recharge_limit_or_zero(v: f64) -> f64 {
    if v.is_finite() && v > 0.0 {
        v
    } else {
        0.0
    }
}

/// 钱包充值生效的最小/最大金额（站点默认货币）。
///
/// 渠道 `recharge_limit_enabled` 时用该渠道的一对数值，否则用全局通用充值设置。
/// 通联子渠道 `allinpay_wechat` / `allinpay_alipay` 共用 `allinpay`。
/// 返回值中 0 表示该侧不限制。
pub fn effective_recharge_amount_limits(
    ui: &PaymentChannelsUiSettings,
    payment_method: &str,
    global_min: f64,
    global_max: f64,
) -> (f64, f64) {
    let ui_id = if payment_method.starts_with("allinpay_") {
        "allinpay"
    } else {
        payment_method
    };
    let (min, max) = ui
        .channels
        .iter()
        .find(|c| c.id == ui_id)
        .filter(|c| c.recharge_limit_enabled)
        .map(|c| (c.min_recharge_amount, c.max_recharge_amount))
        .unwrap_or((global_min, global_max));
    (recharge_limit_or_zero(min), recharge_limit_or_zero(max))
}

/// 单独限额开启时，两侧都大于 0 则最小不能大于最大
pub fn validate_channel_recharge_limits(channels: &[PaymentChannelUiItem]) -> Result<(), String> {
    for c in channels {
        if !c.recharge_limit_enabled {
            continue;
        }
        let min = c.min_recharge_amount;
        let max = c.max_recharge_amount;
        if !min.is_finite() || !max.is_finite() || min < 0.0 || max < 0.0 {
            return Err("充值金额限制不能为负数".to_string());
        }
        if min > 0.0 && max > 0.0 && min > max {
            return Err(format!(
                "渠道 {} 的最小充值金额不能大于最大充值金额",
                c.id
            ));
        }
    }
    Ok(())
}

/// 谷歌 OAuth 2.0 设置
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct GoogleOAuthSettings {
    /// Google OAuth Client ID
    #[serde(default)]
    pub client_id: String,
    /// Google OAuth Client Secret
    #[serde(default)]
    pub client_secret: String,
}

/// 微信开放平台授权登录设置
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct WechatOAuthSettings {
    /// 网站应用 AppId
    #[serde(default)]
    pub app_id: String,
    /// 网站应用密钥 AppSecret
    #[serde(default)]
    pub app_secret: String,
    /// 微信开放平台审核绑定的授权回调域名（如 old.com 或 https://old.com），用于多域名免审中继；留空则跟随当前域名
    #[serde(default)]
    pub callback_domain: String,
}

/// 低余额在途档位：可用额低于 max_available 时限制未完成任务路数；max_available=None 表示其余
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct InflightTier {
    #[serde(default)]
    pub max_available: Option<f64>,
    /// 最大未完成任务路数；**0 = 不限制**
    #[serde(default)]
    pub max_inflight: u32,
}

fn default_inflight_tiers() -> Vec<InflightTier> {
    vec![
        InflightTier {
            max_available: Some(20.0),
            max_inflight: 1,
        },
        InflightTier {
            max_available: Some(50.0),
            max_inflight: 3,
        },
    ]
}

/// 单类别：开关 + 档位
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct InflightCategoryLimit {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_inflight_tiers")]
    pub tiers: Vec<InflightTier>,
}

impl Default for InflightCategoryLimit {
    fn default() -> Self {
        Self {
            enabled: false,
            tiers: default_inflight_tiers(),
        }
    }
}

impl InflightCategoryLimit {
    fn prepared(mut self) -> Self {
        if self.tiers.is_empty() {
            self.tiers = default_inflight_tiers();
        }
        self.tiers
            .sort_by(|a, b| match (a.max_available, b.max_available) {
                (Some(x), Some(y)) => x.partial_cmp(&y).unwrap_or(std::cmp::Ordering::Equal),
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => std::cmp::Ordering::Equal,
            });
        self
    }

    pub fn max_inflight(&self, available: f64) -> Option<u32> {
        if !self.enabled {
            return None;
        }
        for t in &self.tiers {
            let hit = t.max_available.map(|lim| available < lim).unwrap_or(true);
            if hit {
                return (t.max_inflight > 0).then_some(t.max_inflight);
            }
        }
        None
    }
}

/// 在途限制类别（配置与 SQL 条件同一映射）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InflightCat {
    Video,
    Image,
    Chat,
    Other,
}

impl InflightCat {
    #[inline]
    pub fn of(action_type: &str) -> Self {
        match action_type {
            "视频" | "封装" => Self::Video,
            "图片" => Self::Image,
            "聊天" => Self::Chat,
            _ => Self::Other,
        }
    }

    #[inline]
    pub fn count_sql(self) -> &'static str {
        match self {
            Self::Video => "AND action_type = ANY(ARRAY['视频','封装'])",
            Self::Image => "AND action_type = '图片'",
            Self::Chat => "AND action_type = '聊天'",
            Self::Other => {
                "AND COALESCE(action_type, '') NOT IN ('视频','封装','图片','聊天')"
            }
        }
    }
}

/// 视频 / 图片 / 聊天 / 其它
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct InflightLimits {
    #[serde(default)]
    pub video: InflightCategoryLimit,
    #[serde(default)]
    pub image: InflightCategoryLimit,
    #[serde(default)]
    pub chat: InflightCategoryLimit,
    #[serde(default)]
    pub other: InflightCategoryLimit,
}

impl InflightLimits {
    fn prepared(mut self) -> Self {
        self.video = self.video.prepared();
        self.image = self.image.prepared();
        self.chat = self.chat.prepared();
        self.other = self.other.prepared();
        self
    }

    #[inline]
    pub fn get(&self, cat: InflightCat) -> &InflightCategoryLimit {
        match cat {
            InflightCat::Video => &self.video,
            InflightCat::Image => &self.image,
            InflightCat::Chat => &self.chat,
            InflightCat::Other => &self.other,
        }
    }
}

/// Relay 网关设置（管理端；不进公开接口）
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RelaySettings {
    /// 手动轮询是否打上游。关：优先 logs，无缓存再兜底；后台自动轮询不受影响。
    #[serde(default = "default_true")]
    pub manual_poll_upstream: bool,
    /// 后台 TaskPoller 周期（秒）；过短增上游压力，过长延迟结案/退费。默认 30，有效范围 5–300；缺省/0 按 30。
    #[serde(default)]
    pub poll_tick_secs: u64,
    /// 按类别的低余额在途限制
    #[serde(default)]
    pub inflight_limits: InflightLimits,
    /// 模型调用调试日志：开则热路径打到控制台
    #[serde(default)]
    pub enable_debug_log: bool,
    /// 旧字段只读兼容；写入跳过
    #[serde(default, skip_serializing)]
    pub video_inflight_enabled: bool,
    #[serde(default, skip_serializing)]
    pub video_inflight_tiers: Vec<InflightTier>,
}

impl RelaySettings {
    /// 入缓存 / 出管理端前：旧配置迁入；空档补默认；轮询周期钳制
    pub fn prepared(mut self) -> Self {
        self.poll_tick_secs = match self.poll_tick_secs {
            0 => 30,
            n => n.clamp(5, 300),
        };
        // 旧单一开关曾覆盖全部模型 → 四类同档全开
        if self.video_inflight_enabled
            && !self.inflight_limits.video.enabled
            && !self.inflight_limits.image.enabled
            && !self.inflight_limits.chat.enabled
            && !self.inflight_limits.other.enabled
        {
            let tiers = if self.video_inflight_tiers.is_empty() {
                default_inflight_tiers()
            } else {
                std::mem::take(&mut self.video_inflight_tiers)
            };
            for cat in [
                &mut self.inflight_limits.video,
                &mut self.inflight_limits.image,
                &mut self.inflight_limits.chat,
                &mut self.inflight_limits.other,
            ] {
                cat.enabled = true;
                cat.tiers = tiers.clone();
            }
        }
        self.video_inflight_enabled = false;
        self.video_inflight_tiers.clear();
        self.inflight_limits = self.inflight_limits.prepared();
        self
    }
}

impl Default for RelaySettings {
    fn default() -> Self {
        Self {
            manual_poll_upstream: true,
            poll_tick_secs: 30,
            inflight_limits: InflightLimits::default(),
            enable_debug_log: false,
            video_inflight_enabled: false,
            video_inflight_tiers: Vec::new(),
        }
    }
}

/// 对象存储配置（TOS / COS 凭证与默认提供商；不含日志清理）
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct StorageSettings {
    #[serde(default = "default_storage_provider")]
    pub default_provider: String,
    #[serde(default)]
    pub tos_access_key: String,
    #[serde(default)]
    pub tos_secret_key: String,
    #[serde(default)]
    pub tos_endpoint: String,
    #[serde(default)]
    pub tos_region: String,
    #[serde(default)]
    pub tos_bucket: String,
    #[serde(default)]
    pub tos_path_prefix: String,
    #[serde(default)]
    pub tos_custom_domain: String,
    #[serde(default)]
    pub cos_secret_id: String,
    #[serde(default)]
    pub cos_secret_key: String,
    #[serde(default)]
    pub cos_endpoint: String,
    #[serde(default)]
    pub cos_region: String,
    #[serde(default)]
    pub cos_bucket: String,
    #[serde(default)]
    pub cos_path_prefix: String,
    #[serde(default)]
    pub cos_custom_domain: String,
}

impl Default for StorageSettings {
    fn default() -> Self {
        Self {
            default_provider: default_storage_provider(),
            tos_access_key: String::new(),
            tos_secret_key: String::new(),
            tos_endpoint: String::new(),
            tos_region: String::new(),
            tos_bucket: String::new(),
            tos_path_prefix: String::new(),
            tos_custom_domain: String::new(),
            cos_secret_id: String::new(),
            cos_secret_key: String::new(),
            cos_endpoint: String::new(),
            cos_region: String::new(),
            cos_bucket: String::new(),
            cos_path_prefix: String::new(),
            cos_custom_domain: String::new(),
        }
    }
}

fn default_storage_provider() -> String {
    "tos".to_string()
}

/// 日志清理与运维调度设置（与对象存储解耦；旧数据曾写在 storage_settings）
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LogCleanupSettings {
    /// 使用日志详情保留天数，超期自动清理请求/响应内容，0=永不清理
    #[serde(default = "default_log_retention_days")]
    pub log_retention_days: i64,
    /// 使用日志行保留天数：超期行迁入 logs_archive 并从热表删除；0=永不归档（默认）
    #[serde(default = "default_log_row_retention_days")]
    pub log_row_retention_days: i64,
    /// 错误日志保留天数：非 200 状态码错误记录；0=永不清理（默认）
    #[serde(default = "default_error_log_retention_days")]
    pub error_log_retention_days: i64,

    // ===== 5 项重型/核心定时任务错峰执行时间配置（时:分）=====
    /// 1. 每日用量增量统计触发时间（时 0~23，默认 1）
    #[serde(default = "default_daily_stats_hour")]
    pub daily_stats_hour: u32,
    /// 1. 每日用量增量统计触发时间（分 0~59，默认 0）
    #[serde(default)]
    pub daily_stats_minute: u32,

    /// 2. 日志详情大字段清理触发时间（时 0~23，默认 2）
    #[serde(default = "default_clean_hour")]
    pub clean_hour: u32,
    /// 2. 日志详情大字段清理触发时间（分 0~59，默认 30）
    #[serde(default = "default_clean_minute")]
    pub clean_minute: u32,

    /// 3. 日志行冷归档触发时间（时 0~23，默认 3）
    #[serde(default = "default_archive_hour")]
    pub archive_hour: u32,
    /// 3. 日志行冷归档触发时间（分 0~59，默认 30）
    #[serde(default = "default_archive_minute")]
    pub archive_minute: u32,

    /// 4. 错误日志清理触发时间（时 0~23，默认 4）
    #[serde(default = "default_error_clean_hour")]
    pub error_clean_hour: u32,
    /// 4. 错误日志清理触发时间（分 0~59，默认 0）
    #[serde(default)]
    pub error_clean_minute: u32,

    /// 5. 临时存储与插件日志清理触发时间（时 0~23，默认 4）
    #[serde(default = "default_storage_clean_hour")]
    pub storage_clean_hour: u32,
    /// 5. 临时存储与插件日志清理触发时间（分 0~59，默认 30）
    #[serde(default = "default_storage_clean_minute")]
    pub storage_clean_minute: u32,

    // --- 向下兼容旧字段 ---
    /// 每日系统维护总触发时间（时 0~23，默认 3）
    #[serde(default = "default_maintenance_hour")]
    pub maintenance_hour: u32,
    /// 每日系统维护总触发时间（分 0~59，默认 0）
    #[serde(default)]
    pub maintenance_minute: u32,
}

impl Default for LogCleanupSettings {
    fn default() -> Self {
        Self {
            log_retention_days: default_log_retention_days(),
            log_row_retention_days: default_log_row_retention_days(),
            error_log_retention_days: default_error_log_retention_days(),
            daily_stats_hour: default_daily_stats_hour(),
            daily_stats_minute: 0,
            clean_hour: default_clean_hour(),
            clean_minute: default_clean_minute(),
            archive_hour: default_archive_hour(),
            archive_minute: default_archive_minute(),
            error_clean_hour: default_error_clean_hour(),
            error_clean_minute: 0,
            storage_clean_hour: default_storage_clean_hour(),
            storage_clean_minute: default_storage_clean_minute(),
            maintenance_hour: default_maintenance_hour(),
            maintenance_minute: 0,
        }
    }
}

impl LogCleanupSettings {
    pub fn from_legacy_storage_json(v: &serde_json::Value) -> Self {
        let mut s = Self::default();
        if let Some(n) = v.get("log_retention_days").and_then(|x| x.as_i64()) {
            s.log_retention_days = n;
        }
        if let Some(n) = v.get("log_row_retention_days").and_then(|x| x.as_i64()) {
            s.log_row_retention_days = n;
        }
        if let Some(n) = v.get("error_log_retention_days").and_then(|x| x.as_i64()) {
            s.error_log_retention_days = n;
        }
        s
    }

    /// 严格防越界兜底的每日用量统计执行时间 (时 0..=23, 分 0..=59)
    pub fn safe_daily_stats_hm(&self) -> (u32, u32) {
        (self.daily_stats_hour % 24, self.daily_stats_minute % 60)
    }

    /// 严格防越界兜底的日志详情大字段清理执行时间 (时 0..=23, 分 0..=59)
    pub fn safe_clean_hm(&self) -> (u32, u32) {
        (self.clean_hour % 24, self.clean_minute % 60)
    }

    /// 严格防越界兜底的日志行冷归档执行时间 (时 0..=23, 分 0..=59)
    pub fn safe_archive_hm(&self) -> (u32, u32) {
        (self.archive_hour % 24, self.archive_minute % 60)
    }

    /// 严格防越界兜底的超期错误日志清理执行时间 (时 0..=23, 分 0..=59)
    pub fn safe_error_clean_hm(&self) -> (u32, u32) {
        (self.error_clean_hour % 24, self.error_clean_minute % 60)
    }

    /// 严格防越界兜底的临时存储与插件日志清理执行时间 (时 0..=23, 分 0..=59)
    pub fn safe_storage_clean_hm(&self) -> (u32, u32) {
        (self.storage_clean_hour % 24, self.storage_clean_minute % 60)
    }
}

fn default_log_retention_days() -> i64 {
    30
}

fn default_log_row_retention_days() -> i64 {
    0
}

fn default_error_log_retention_days() -> i64 {
    0
}

fn default_daily_stats_hour() -> u32 {
    1
}

fn default_clean_hour() -> u32 {
    2
}

fn default_clean_minute() -> u32 {
    30
}

fn default_archive_hour() -> u32 {
    3
}

fn default_archive_minute() -> u32 {
    30
}

fn default_error_clean_hour() -> u32 {
    4
}

fn default_storage_clean_hour() -> u32 {
    4
}

fn default_storage_clean_minute() -> u32 {
    30
}

fn default_maintenance_hour() -> u32 {
    3
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MenuItemConfig {
    #[serde(default)]
    pub key: String,
    #[serde(default)]
    pub label_zh: String,
    #[serde(default)]
    pub label_en: String,
    #[serde(default)]
    pub icon: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub sort_order: i64,
    #[serde(default = "default_all_levels")]
    pub allowed_levels: String,
}

fn default_all_levels() -> String {
    "all".to_string()
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct MenuConfigSettings {
    #[serde(default)]
    pub items: Vec<MenuItemConfig>,
}

/// 提示通知设置
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct NotificationSettings {
    #[serde(default)]
    pub site_notification_enabled: bool,
    /// 是否向用户开放短信余额提醒订阅
    #[serde(default)]
    pub sms_balance_notification: bool,
    /// 是否向用户开放邮件余额提醒订阅
    #[serde(default)]
    pub email_balance_notification: bool,
    /// 是否向用户开放站内 Web 通知订阅
    #[serde(default = "default_true_notif")]
    pub web_notification_enabled: bool,
    /// 是否向用户开放浏览器 Push 订阅
    #[serde(default = "default_true_notif")]
    pub push_notification_enabled: bool,
    /// 是否向用户开放勿扰模式
    #[serde(default = "default_true_notif")]
    pub do_not_disturb_enabled: bool,
    #[serde(default = "default_low_balance_threshold")]
    pub low_balance_threshold: f64,
    /// 余额不足提醒邮件主题（支持 {{site_name}} {{balance}} {{threshold}}）
    #[serde(default = "default_low_balance_email_subject")]
    pub low_balance_email_subject: String,
    /// 余额不足提醒邮件 HTML 正文（支持 {{site_name}} {{balance}} {{threshold}}）
    #[serde(default = "default_low_balance_email_html")]
    pub low_balance_email_html: String,
}

fn default_true_notif() -> bool {
    true
}

fn default_low_balance_threshold() -> f64 {
    100.0
}

pub fn default_low_balance_email_subject() -> String {
    "【{{site_name}}】账户余额不足提醒".to_string()
}

pub fn default_low_balance_email_html() -> String {
    r#"<div style="font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, 'Helvetica Neue', Arial, sans-serif; max-width: 600px; margin: 0 auto; border: 1px solid #e8e8e8; border-radius: 8px;">
  <div style="padding: 30px;">
    <h2 style="color: #fa8c16; margin: 0 0 24px 0; font-size: 22px; font-weight: 600;">余额不足提醒</h2>
    <p style="color: #333; font-size: 16px; margin: 0 0 16px 0;">您好！</p>
    <p style="color: #333; font-size: 16px; margin: 0 0 24px 0;">您的账户可用余额已低于设定阈值，请及时充值以免影响服务使用。</p>
    <div style="background-color: #f5f5f5; padding: 20px; border-radius: 6px; margin-bottom: 24px;">
      <p style="color: #666; font-size: 14px; margin: 0 0 8px 0;">当前余额：<strong style="color: #fa541c; font-size: 18px;">{{balance}}</strong></p>
      <p style="color: #666; font-size: 14px; margin: 0;">提醒阈值：<strong>{{threshold}}</strong></p>
    </div>
    <div style="border-top: 1px dashed #e8e8e8; margin-top: 24px; padding-top: 16px;">
      <p style="color: #999; font-size: 12px; margin: 0;">此邮件由 {{site_name}} 系统根据您的通知订阅设置自动发送。</p>
    </div>
  </div>
</div>"#
    .to_string()
}

/// 渲染余额提醒模版变量
pub fn render_low_balance_template(
    template: &str,
    site_name: &str,
    balance: &str,
    threshold: &str,
) -> String {
    template
        .replace("{{site_name}}", site_name)
        .replace("{{balance}}", balance)
        .replace("{{threshold}}", threshold)
}

impl Default for NotificationSettings {
    fn default() -> Self {
        Self {
            site_notification_enabled: false,
            sms_balance_notification: false,
            email_balance_notification: false,
            web_notification_enabled: true,
            push_notification_enabled: true,
            do_not_disturb_enabled: true,
            low_balance_threshold: 100.0,
            low_balance_email_subject: default_low_balance_email_subject(),
            low_balance_email_html: default_low_balance_email_html(),
        }
    }
}

/// 销方开票主体配置（支持多主体及普票/专票资质配置）
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SellerInvoiceSubject {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub company_name: String,
    #[serde(default)]
    pub tax_id: String,
    #[serde(default)]
    pub contact_email: Option<String>,
    #[serde(default)]
    pub company_address: Option<String>,
    #[serde(default)]
    pub company_phone: Option<String>,
    #[serde(default, alias = "website")]
    pub company_website: Option<String>,
    #[serde(default)]
    pub bank_name: Option<String>,
    #[serde(default)]
    pub bank_account: Option<String>,
    /// 是否支持增值税普通发票
    #[serde(default = "default_true")]
    pub support_normal: bool,
    /// 是否支持增值税专用发票
    #[serde(default)]
    pub support_special: bool,
    /// 是否为默认开票主体
    #[serde(default)]
    pub is_default: bool,
    /// 启用状态
    #[serde(default = "default_true")]
    pub enabled: bool,
}

/// 发票与收据全局设置
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct InvoiceSettings {
    #[serde(default = "default_true")]
    pub invoice_enabled: bool,
    #[serde(default = "default_invoice_mode")]
    pub invoice_mode: String,
    #[serde(default = "default_invoice_company_name")]
    pub invoice_company_name: String,
    #[serde(default = "default_invoice_tax_id")]
    pub invoice_tax_id: String,
    #[serde(default = "default_invoice_contact_email")]
    pub invoice_contact_email: String,
    #[serde(default = "default_invoice_company_address")]
    pub invoice_company_address: String,
    #[serde(default)]
    pub invoice_company_phone: String,
    #[serde(default, alias = "invoice_website")]
    pub invoice_company_website: String,
    #[serde(default = "default_invoice_currency_symbol")]
    pub invoice_currency_symbol: String,
    #[serde(default = "default_invoice_prefix")]
    pub invoice_prefix: String,
    #[serde(default = "default_receipt_prefix")]
    pub receipt_prefix: String,
    #[serde(default = "default_invoice_notes")]
    pub invoice_notes: String,
    #[serde(default = "default_true")]
    pub invoice_show_invoices: bool,
    #[serde(default = "default_true")]
    pub invoice_show_receipts: bool,
    #[serde(default = "default_china_min_amount")]
    pub china_min_amount: f64,
    #[serde(default = "default_china_invoice_content")]
    pub china_invoice_content: String,
    #[serde(default = "default_china_tax_rate")]
    pub china_tax_rate: f64,
    #[serde(default = "default_china_invoice_notice")]
    pub china_invoice_notice: String,
    /// 多开票主体与销方资质列表
    #[serde(default)]
    pub invoice_sellers: Vec<SellerInvoiceSubject>,
}

pub fn default_invoice_mode() -> String {
    "international".to_string()
}
pub fn default_invoice_company_name() -> String {
    String::new()
}
pub fn default_invoice_tax_id() -> String {
    String::new()
}
pub fn default_invoice_contact_email() -> String {
    String::new()
}
pub fn default_invoice_company_address() -> String {
    String::new()
}
pub fn default_invoice_currency_symbol() -> String {
    "$".to_string()
}
pub fn default_invoice_prefix() -> String {
    "INV-".to_string()
}
pub fn default_receipt_prefix() -> String {
    "REC-".to_string()
}
pub fn default_invoice_notes() -> String {
    "Thank you for your business! Payment has been processed in full.".to_string()
}
pub fn default_china_min_amount() -> f64 {
    100.0
}
pub fn default_china_invoice_content() -> String {
    "*信息技术服务*软件技术服务费".to_string()
}
pub fn default_china_tax_rate() -> f64 {
    6.0
}
pub fn default_china_invoice_notice() -> String {
    "增值税普通发票将在审核通过后发送至填写的邮箱。".to_string()
}

impl InvoiceSettings {
    /// 历史代码曾把旧金山样例销方写入默认配置；商用环境不得印到正式发票上。
    pub fn clear_placeholder_issuer(&mut self) {
        const DEMO_NAME: &str = "TokensByte Inc.";
        const DEMO_TAX: &str = "US-987654321";
        const DEMO_EMAIL: &str = "billing@tokensbyte.com";
        const DEMO_ADDR: &str = "100 Innovation Way, Suite 300, San Francisco, CA 94107";
        let is_demo = self.invoice_tax_id == DEMO_TAX
            || self.invoice_company_address == DEMO_ADDR
            || self.invoice_contact_email == DEMO_EMAIL;
        if is_demo {
            if self.invoice_company_name == DEMO_NAME {
                self.invoice_company_name.clear();
            }
            if self.invoice_tax_id == DEMO_TAX {
                self.invoice_tax_id.clear();
            }
            if self.invoice_company_address == DEMO_ADDR {
                self.invoice_company_address.clear();
            }
            if self.invoice_contact_email == DEMO_EMAIL {
                self.invoice_contact_email.clear();
            }
        }
        self.invoice_sellers.retain(|seller| {
            seller.tax_id != DEMO_TAX
                && seller.company_address.as_deref() != Some(DEMO_ADDR)
                && seller.contact_email.as_deref() != Some(DEMO_EMAIL)
        });
    }

    /// 获取当前有效的开票主体列表，如果未配置 invoice_sellers 则根据旧配置自动生成默认主体
    pub fn get_effective_sellers(&self) -> Vec<SellerInvoiceSubject> {
        if !self.invoice_sellers.is_empty() {
            return self.invoice_sellers.clone();
        }
        if !self.invoice_company_name.is_empty() {
            vec![SellerInvoiceSubject {
                id: "default_seller".to_string(),
                company_name: self.invoice_company_name.clone(),
                tax_id: self.invoice_tax_id.clone(),
                contact_email: if self.invoice_contact_email.is_empty() {
                    None
                } else {
                    Some(self.invoice_contact_email.clone())
                },
                company_address: if self.invoice_company_address.is_empty() {
                    None
                } else {
                    Some(self.invoice_company_address.clone())
                },
                company_phone: if self.invoice_company_phone.is_empty() {
                    None
                } else {
                    Some(self.invoice_company_phone.clone())
                },
                company_website: if self.invoice_company_website.is_empty() {
                    None
                } else {
                    Some(self.invoice_company_website.clone())
                },
                bank_name: None,
                bank_account: None,
                support_normal: true,
                support_special: true,
                is_default: true,
                enabled: true,
            }]
        } else {
            Vec::new()
        }
    }
}

impl Default for InvoiceSettings {
    fn default() -> Self {
        Self {
            invoice_enabled: true,
            invoice_mode: default_invoice_mode(),
            invoice_company_name: default_invoice_company_name(),
            invoice_tax_id: default_invoice_tax_id(),
            invoice_contact_email: default_invoice_contact_email(),
            invoice_company_address: default_invoice_company_address(),
            invoice_company_phone: String::new(),
            invoice_company_website: String::new(),
            invoice_currency_symbol: default_invoice_currency_symbol(),
            invoice_prefix: default_invoice_prefix(),
            receipt_prefix: default_receipt_prefix(),
            invoice_notes: default_invoice_notes(),
            invoice_show_invoices: true,
            invoice_show_receipts: true,
            china_min_amount: default_china_min_amount(),
            china_invoice_content: default_china_invoice_content(),
            china_tax_rate: default_china_tax_rate(),
            china_invoice_notice: default_china_invoice_notice(),
            invoice_sellers: Vec::new(),
        }
    }
}

/// 聚合所有设置（读取）
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AllSettings {
    #[serde(default)]
    pub site: SiteSettings,
    #[serde(default)]
    pub currency: CurrencySettings,
    #[serde(default)]
    pub login: LoginSettings,
    #[serde(default)]
    pub registration: RegistrationSettings,
    #[serde(default)]
    pub smtp: SMTPSettings,
    #[serde(default)]
    pub sms: Option<SmsSettings>,
    #[serde(default)]
    pub marketing: MarketingSettings,
    #[serde(default)]
    pub database: DatabaseSettings,
    #[serde(default)]
    pub payment_wechat: Option<PaymentWechatSettings>,
    #[serde(default)]
    pub payment_alipay: Option<PaymentAlipaySettings>,
    #[serde(default)]
    pub payment_stripe: Option<PaymentStripeSettings>,
    #[serde(default)]
    pub payment_hyperbc: Option<PaymentHyperbcSettings>,
    #[serde(default)]
    pub payment_allinpay: Option<PaymentAllinpaySettings>,
    /// 支付渠道展示/排序/分渠道开关（与密钥配置解耦）
    #[serde(default)]
    pub payment_channels_ui: Option<PaymentChannelsUiSettings>,
    #[serde(default)]
    pub google_oauth: Option<GoogleOAuthSettings>,
    #[serde(default)]
    pub wechat_oauth: Option<WechatOAuthSettings>,
    #[serde(default)]
    pub agreement: AgreementSettings,
    #[serde(default)]
    pub storage: Option<StorageSettings>,
    #[serde(default)]
    pub log_cleanup: Option<LogCleanupSettings>,
    #[serde(default)]
    pub menu_config: Option<MenuConfigSettings>,
    #[serde(default)]
    pub notification: NotificationSettings,
    #[serde(default)]
    pub relay: RelaySettings,
    #[serde(default)]
    pub invoices: Option<InvoiceSettings>,
    #[serde(default, skip_deserializing)]
    pub server_timezone: Option<String>,
    #[serde(default, skip_deserializing)]
    pub server_time: Option<String>,
}

/// 更新设置请求（写入）
#[derive(Debug, Serialize, Deserialize)]
pub struct UpdateSettingsRequest {
    #[serde(default)]
    pub site: Option<serde_json::Value>,
    #[serde(default)]
    pub currency: Option<serde_json::Value>,
    #[serde(default)]
    pub login: Option<serde_json::Value>,
    #[serde(default)]
    pub registration: Option<serde_json::Value>,
    #[serde(default)]
    pub smtp: Option<serde_json::Value>,
    #[serde(default)]
    pub sms: Option<serde_json::Value>,
    #[serde(default)]
    pub marketing: Option<serde_json::Value>,
    #[serde(default)]
    pub database: Option<serde_json::Value>,
    #[serde(default)]
    pub payment_wechat: Option<serde_json::Value>,
    #[serde(default)]
    pub payment_alipay: Option<serde_json::Value>,
    #[serde(default)]
    pub payment_stripe: Option<serde_json::Value>,
    #[serde(default)]
    pub payment_hyperbc: Option<serde_json::Value>,
    #[serde(default)]
    pub payment_allinpay: Option<serde_json::Value>,
    #[serde(default)]
    pub payment_channels_ui: Option<serde_json::Value>,
    #[serde(default)]
    pub google_oauth: Option<serde_json::Value>,
    #[serde(default)]
    pub wechat_oauth: Option<serde_json::Value>,
    #[serde(default)]
    pub agreement: Option<serde_json::Value>,
    #[serde(default)]
    pub storage: Option<serde_json::Value>,
    #[serde(default)]
    pub log_cleanup: Option<serde_json::Value>,
    #[serde(default)]
    pub menu_config: Option<serde_json::Value>,
    #[serde(default)]
    pub notification: Option<serde_json::Value>,
    #[serde(default)]
    pub relay: Option<serde_json::Value>,
    #[serde(default)]
    pub invoices: Option<serde_json::Value>,
}

// ════════════════════════════════════════════════════════════════════════════
// 【安全原则】公开接口返回的数据结构
//
// 以下 PublicSettings 系列结构体用于无需认证的公开接口返回值。
// 系统安全原则：隐私数据（密钥、密码、Secret、数据库信息等）绝不暴露到公开接口。
// 新增设置字段时，须评估是否属于公开数据。如为隐私数据，仅添加到 AllSettings，
// 不得添加到 PublicSettings。此原则必须被所有开发者（包括 AI）严格遵守。
// ════════════════════════════════════════════════════════════════════════════

/// 公开注册设置 — 仅暴露注册方式开关与绑定策略，隐藏 IP 限制、邮箱白名单等防刷细节
#[derive(Debug, Serialize, Clone)]
pub struct PublicRegistrationSettings {
    #[serde(default)]
    pub enable_username_registration: bool,
    #[serde(default)]
    pub enable_email_registration: bool,
    #[serde(default)]
    pub enable_mobile_registration: bool,
    #[serde(default)]
    pub enable_password_recovery: bool,
    #[serde(default)]
    pub require_bind_mobile: bool,
    #[serde(default)]
    pub require_bind_email: bool,
    #[serde(default = "default_bind_enforcement")]
    pub bind_enforcement: String,
    #[serde(default)]
    pub enable_user_kyc: bool,
}

impl From<&RegistrationSettings> for PublicRegistrationSettings {
    fn from(r: &RegistrationSettings) -> Self {
        Self {
            enable_username_registration: r.enable_username_registration,
            enable_email_registration: r.enable_email_registration,
            enable_mobile_registration: r.enable_mobile_registration,
            enable_password_recovery: r.enable_password_recovery,
            require_bind_mobile: r.require_bind_mobile,
            require_bind_email: r.require_bind_email,
            bind_enforcement: if r.bind_enforcement.is_empty() {
                default_bind_enforcement()
            } else {
                r.bind_enforcement.clone()
            },
            enable_user_kyc: r.enable_user_kyc,
        }
    }
}

/// 公开营销设置 — 仅暴露注册赠送 / 兑换开关，隐藏具体金额配置
#[derive(Debug, Serialize, Clone)]
pub struct PublicMarketingSettings {
    #[serde(default)]
    pub enable_registration_gift: bool,
    #[serde(default)]
    pub enable_redemption: bool,
}

impl From<&MarketingSettings> for PublicMarketingSettings {
    fn from(m: &MarketingSettings) -> Self {
        Self {
            enable_registration_gift: m.enable_registration_gift,
            enable_redemption: m.enable_redemption,
        }
    }
}

/// 公开支付状态 — 仅暴露各支付渠道的启用开关，不含任何密钥/密码/私钥
#[derive(Debug, Serialize, Clone, Default)]
pub struct PublicPaymentStatus {
    #[serde(default)]
    pub wechat_enabled: bool,
    #[serde(default)]
    pub alipay_enabled: bool,
    #[serde(default)]
    pub stripe_enabled: bool,
    #[serde(default)]
    pub hyperbc_enabled: bool,
    #[serde(default)]
    pub allinpay_enabled: bool,
}

/// 公开通知设置
#[derive(Debug, Serialize, Clone)]
pub struct PublicNotificationSettings {
    #[serde(default)]
    pub site_notification_enabled: bool,
    #[serde(default)]
    pub sms_balance_notification: bool,
    #[serde(default)]
    pub email_balance_notification: bool,
    #[serde(default)]
    pub web_notification_enabled: bool,
    #[serde(default)]
    pub push_notification_enabled: bool,
    #[serde(default)]
    pub do_not_disturb_enabled: bool,
    #[serde(default)]
    pub low_balance_threshold: f64,
}

impl From<&NotificationSettings> for PublicNotificationSettings {
    fn from(n: &NotificationSettings) -> Self {
        Self {
            site_notification_enabled: n.site_notification_enabled,
            sms_balance_notification: n.sms_balance_notification,
            email_balance_notification: n.email_balance_notification,
            web_notification_enabled: n.web_notification_enabled,
            push_notification_enabled: n.push_notification_enabled,
            do_not_disturb_enabled: n.do_not_disturb_enabled,
            low_balance_threshold: if n.low_balance_threshold > 0.0 {
                n.low_balance_threshold
            } else {
                100.0
            },
        }
    }
}

/// 公开设置聚合 — 仅包含前端 UI 渲染所需的安全数据
///
/// 【安全】不包含任何密钥、密码、Secret、数据库、支付、SMTP、短信、存储等隐私配置。
/// OAuth 仅暴露 client_id / app_id（前端发起 OAuth 跳转必需），不暴露 secret。
#[derive(Debug, Serialize, Clone)]
pub struct PublicSettings {
    #[serde(default)]
    pub is_open_source: bool,
    #[serde(default)]
    pub site: SiteSettings,
    #[serde(default)]
    pub currency: CurrencySettings,
    #[serde(default)]
    pub login: LoginSettings,
    #[serde(default)]
    pub registration: PublicRegistrationSettings,
    #[serde(default)]
    pub marketing: PublicMarketingSettings,
    /// 各支付渠道启用状态（仅布尔值，不含密钥；兼容旧前端）
    #[serde(default)]
    pub payment: PublicPaymentStatus,
    /// 支付渠道列表（排序/展示名/副标题/logo/启用，不含密钥）
    #[serde(default)]
    pub payment_channels: Vec<PublicPaymentChannel>,
    #[serde(default)]
    pub agreement: AgreementSettings,
    /// 微信 OAuth app_id（前端扫码绑定/登录需要），不含 app_secret
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub wechat_oauth_app_id: Option<String>,
    /// 微信 OAuth 授权回调主域名源（带 scheme，如 https://old.com），用于多域名免审中继
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub wechat_oauth_callback_domain: Option<String>,
    /// Google OAuth client_id（前端 OAuth 跳转需要），不含 client_secret
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub google_oauth_client_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub menu_config: Option<MenuConfigSettings>,
    #[serde(default)]
    pub notification: PublicNotificationSettings,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub invoices: Option<InvoiceSettings>,
}
