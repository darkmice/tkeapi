const http = require('http');
const fs = require('fs');
const path = require('path');

function htmlEscape(str) {
  return String(str || '')
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#39;');
}

function buildStaticHtml(title, content, copyright, lang, pageType, siteTitle) {
  const isZh = lang === 'zh';
  const htmlLang = isZh ? 'zh-CN' : 'en';
  const footerHtml = copyright ? `<footer class="footer">${htmlEscape(copyright)}</footer>` : '';
  const normalizedContent = String(content || '').replace(/&nbsp;/g, ' ');
  const cleanSiteTitle = String(siteTitle || '').trim();
  const headTitle = cleanSiteTitle ? `${title}-${cleanSiteTitle}` : title;

  return `<!DOCTYPE html>
<html lang="${htmlLang}">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>${htmlEscape(headTitle)}</title>
  <style>
    *, *::before, *::after { box-sizing: border-box; margin: 0; padding: 0; }
    html, body {
      width: 100%;
      overflow-x: hidden;
    }
    body {
      min-height: 100vh;
      background-color: #0a0a0a;
      color: #e5e5e5;
      font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, "Helvetica Neue", Arial, sans-serif;
      line-height: 1.8;
      display: flex;
      flex-direction: column;
      align-items: center;
      -webkit-font-smoothing: antialiased;
    }
    .container {
      max-width: 860px;
      width: 100%;
      margin: 0 auto;
      padding: 50px 24px 40px;
      flex: 1;
      box-sizing: border-box;
    }
    .page-title {
      color: #ffffff;
      font-size: 26px;
      font-weight: 600;
      text-align: center;
      margin-bottom: 36px;
      letter-spacing: -0.01em;
      word-break: break-word;
      overflow-wrap: break-word;
    }
    .ql-editor {
      font-size: 15px;
      line-height: 1.85;
      color: rgba(255, 255, 255, 0.85);
      word-break: break-word;
      overflow-wrap: anywhere;
      word-wrap: break-word;
      white-space: normal;
      width: 100%;
      max-width: 100%;
      box-sizing: border-box;
    }
    .ql-editor h1, .ql-editor h2, .ql-editor h3, .ql-editor h4 {
      color: #ffffff;
      margin-top: 30px;
      margin-bottom: 12px;
      font-weight: 600;
      word-break: break-word;
      overflow-wrap: anywhere;
      white-space: normal;
    }
    .ql-editor h1 { font-size: 24px; }
    .ql-editor h2 { font-size: 20px; }
    .ql-editor h3 { font-size: 17px; }
    .ql-editor h4 { font-size: 15px; }
    .ql-editor p {
      margin-bottom: 14px;
      word-break: break-word;
      overflow-wrap: anywhere;
      white-space: normal;
    }
    .ql-editor ul, .ql-editor ol {
      padding-left: 24px;
      margin-bottom: 16px;
      word-break: break-word;
      overflow-wrap: anywhere;
      white-space: normal;
    }
    .ql-editor li {
      margin-bottom: 6px;
      word-break: break-word;
      overflow-wrap: anywhere;
      white-space: normal;
    }
    .ql-editor strong {
      color: #ffffff;
      font-weight: 600;
    }
    .ql-editor a {
      color: #60a5fa;
      text-decoration: underline;
      word-break: break-all;
      overflow-wrap: anywhere;
    }
    .ql-editor a:hover {
      color: #93c5fd;
    }
    .ql-editor blockquote {
      border-left: 3px solid #3b82f6;
      padding: 10px 16px;
      margin: 18px 0;
      color: rgba(255, 255, 255, 0.7);
      background: rgba(255, 255, 255, 0.03);
      border-radius: 0 6px 6px 0;
      word-break: break-word;
      overflow-wrap: anywhere;
      white-space: normal;
    }
    .ql-editor table {
      width: 100%;
      border-collapse: collapse;
      margin: 18px 0;
      font-size: 14px;
      word-break: break-word;
    }
    .ql-editor th, .ql-editor td {
      border: 1px solid rgba(255, 255, 255, 0.12);
      padding: 10px 14px;
      word-break: break-word;
      overflow-wrap: anywhere;
    }
    .ql-editor th {
      background: rgba(255, 255, 255, 0.05);
      font-weight: 600;
      color: #ffffff;
    }
    .footer {
      width: 100%;
      text-align: center;
      font-size: 13px;
      line-height: 1.6;
      color: rgba(255, 255, 255, 0.45);
      padding: 24px 24px 36px;
      user-select: none;
      border-top: 1px solid rgba(255, 255, 255, 0.06);
      box-sizing: border-box;
    }
    @media (max-width: 640px) {
      .container { padding: 32px 16px 24px; }
      .page-title { font-size: 21px; margin-bottom: 24px; }
    }
  </style>
</head>
<body>
  <div class="container">
    <h1 class="page-title">${htmlEscape(title)}</h1>
    <div class="ql-editor">
      ${normalizedContent}
    </div>
  </div>
  ${footerHtml}
</body>
</html>`;
}

function buildRedirectHtml(title, targetUrl, siteTitle) {
  let cleanUrl = String(targetUrl || '').trim();
  if (!cleanUrl.startsWith('http://') && !cleanUrl.startsWith('https://') && !cleanUrl.startsWith('/')) {
    cleanUrl = 'https://' + cleanUrl;
  }
  const cleanSiteTitle = String(siteTitle || '').trim();
  const headTitle = cleanSiteTitle ? `${title}-${cleanSiteTitle}` : title;

  return `<!DOCTYPE html>
<html>
<head>
  <meta charset="utf-8">
  <meta http-equiv="refresh" content="0;url=${htmlEscape(cleanUrl)}">
  <title>${htmlEscape(headTitle)}</title>
  <script>window.location.replace("${htmlEscape(cleanUrl)}");</script>
  <style>
    body {
      background: #0a0a0a;
      color: rgba(255, 255, 255, 0.6);
      font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
      display: flex;
      align-items: center;
      justify-content: center;
      height: 100vh;
      margin: 0;
    }
    a { color: #3b82f6; text-decoration: underline; }
  </style>
</head>
<body>
  <p>正在跳转至 <a href="${htmlEscape(cleanUrl)}">${htmlEscape(title)}</a> ...</p>
</body>
</html>`;
}

http.get('http://localhost:3000/api/v1/settings', (res) => {
  let data = '';
  res.on('data', chunk => { data += chunk; });
  res.on('end', () => {
    try {
      const json = JSON.parse(data);
      const agreement = json.agreement || {};
      const copyright = (json.site?.copyright || '').trim();
      const siteTitle = (json.site?.title || '').trim();

      const termsZh = (agreement.tos_mode === 'link' && agreement.tos_link)
        ? buildRedirectHtml('服务条款', agreement.tos_link, siteTitle)
        : buildStaticHtml('服务条款 (Terms of Service)', agreement.tos_content || '', copyright, 'zh', 'terms', siteTitle);

      const termsEn = (agreement.tos_mode_en === 'link' && (agreement.tos_link_en || agreement.tos_link))
        ? buildRedirectHtml('Terms of Service', agreement.tos_link_en || agreement.tos_link, siteTitle)
        : buildStaticHtml('Terms of Service', agreement.tos_content_en || agreement.tos_content || '', copyright, 'en', 'terms', siteTitle);

      const privacyZh = (agreement.privacy_mode === 'link' && agreement.privacy_link)
        ? buildRedirectHtml('隐私政策', agreement.privacy_link, siteTitle)
        : buildStaticHtml('隐私政策 (Privacy Policy)', agreement.privacy_content || '', copyright, 'zh', 'privacy', siteTitle);

      const privacyEn = (agreement.privacy_mode_en === 'link' && (agreement.privacy_link_en || agreement.privacy_link))
        ? buildRedirectHtml('Privacy Policy', agreement.privacy_link_en || agreement.privacy_link, siteTitle)
        : buildStaticHtml('Privacy Policy', agreement.privacy_content_en || agreement.privacy_content || '', copyright, 'en', 'privacy', siteTitle);

      const baseDir = path.resolve(__dirname, '..');
      const dirs = [
        path.join(baseDir, 'backend/data/legal'),
        path.join(baseDir, 'frontend/public/legal')
      ];

      dirs.forEach(d => {
        fs.mkdirSync(d, { recursive: true });
        fs.writeFileSync(path.join(d, 'terms_zh.html'), termsZh, 'utf8');
        fs.writeFileSync(path.join(d, 'terms.html'), termsZh, 'utf8');
        fs.writeFileSync(path.join(d, 'tos_zh.html'), termsZh, 'utf8');
        fs.writeFileSync(path.join(d, 'tos.html'), termsZh, 'utf8');
        fs.writeFileSync(path.join(d, 'terms_en.html'), termsEn, 'utf8');
        fs.writeFileSync(path.join(d, 'tos_en.html'), termsEn, 'utf8');
        fs.writeFileSync(path.join(d, 'privacy_zh.html'), privacyZh, 'utf8');
        fs.writeFileSync(path.join(d, 'privacy.html'), privacyZh, 'utf8');
        fs.writeFileSync(path.join(d, 'privacy_en.html'), privacyEn, 'utf8');
        console.log('Successfully generated static legal pages to:', d);
      });
    } catch (e) {
      console.error('Error generating static legal pages:', e);
      process.exit(1);
    }
  });
}).on('error', (err) => {
  console.error('Failed to fetch settings from port 3000:', err.message);
  process.exit(1);
});
