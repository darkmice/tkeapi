/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia 
 * @license        MIT (https://www.tkeapi.com/)
 */

import { defineConfig, loadEnv } from 'vite'
import react from '@vitejs/plugin-react'
import tailwindcss from '@tailwindcss/vite'
import fs from 'node:fs'
import path from 'node:path'

// https://vite.dev/config/
// Docker: VITE_API_TARGET=http://backend:3000；本地多实例由 dev 脚本注入
const apiTarget = process.env.VITE_API_TARGET || 'http://127.0.0.1:3000'
const frontendPort = Number(process.env.FRONTEND_PORT || process.env.VITE_PORT || 5173) || 5173
const isSymlinkNodeModules = (() => {
  try {
    return fs.lstatSync(path.resolve(__dirname, 'node_modules')).isSymbolicLink()
  } catch {
    return false
  }
})()
const viteCacheSubdir = isSymlinkNodeModules ? '.vite-opensource' : '.vite'

export default defineConfig(({ mode }) => {
  const env = loadEnv(mode, process.cwd(), '')

  return {
    cacheDir: `node_modules/${viteCacheSubdir}`,
    plugins: [
      {
        name: 'vite-plugin-deps-fallback',
        configureServer(server) {
          server.middlewares.use((req, res, next) => {
            if (req.url && req.url.includes(`/node_modules/${viteCacheSubdir}/deps/`)) {
              const cleanUrl = req.url.split('?')[0];
              const fileName = path.basename(cleanUrl);
              const depsDir = path.resolve(__dirname, `node_modules/${viteCacheSubdir}/deps`);
              const targetPath = path.join(depsDir, fileName);

              if (!fs.existsSync(targetPath)) {
                // 当请求已失效的旧哈希依赖 chunk 时，返回 504 Outdated Optimize Dep 触发客户端自动整页刷新，避免混用多套 React 实例
                res.statusCode = 504;
                res.statusMessage = 'Outdated Optimize Dep';
                res.end('Outdated Optimize Dep');
                return;
              }
              res.setHeader('Cache-Control', 'no-cache, no-store, must-revalidate');
            }
            next();
          });
        },
      },
      {
        name: 'vite-plugin-legal-static-pages',
        configureServer(server) {
          server.middlewares.use((req, res, next) => {
            if (req.url) {
              const [pathname, search] = req.url.split('?');
              if (pathname.startsWith('/legal/')) {
                const sub = pathname.replace(/^\/legal\//, '');
                let fileType = '';
                if (sub.startsWith('terms') || sub.startsWith('tos')) fileType = 'terms';
                else if (sub.startsWith('privacy')) fileType = 'privacy';

                if (fileType) {
                  const params = new URLSearchParams(search || '');
                  const requested = (params.get('lang') || '').toLowerCase();
                  const lang = sub.includes('_en')
                    ? 'en'
                    : requested
                      ? (requested.startsWith('zh') ? 'zh' : 'en')
                      : 'zh';
                  const staticFilePath = path.resolve(__dirname, `public/legal/${fileType}_${lang}.html`);
                  if (fs.existsSync(staticFilePath)) {
                    res.setHeader('Content-Type', 'text/html; charset=utf-8');
                    res.setHeader('Cache-Control', 'no-cache, no-store, must-revalidate');
                    res.end(fs.readFileSync(staticFilePath));
                    return;
                  }
                }
              }
            }
            next();
          });
        },
      },
      react(),
      tailwindcss(),
    ],
    define: {
      'import.meta.env.VITE_ENABLE_PLUGINS': JSON.stringify(
        env.VITE_ENABLE_PLUGINS ?? 'true'
      ),
    },
    resolve: {
      dedupe: ['react', 'react-dom', 'react-is', 'use-sync-external-store'],
    },
    server: {
      host: '0.0.0.0',
      port: frontendPort,
      strictPort: Boolean(process.env.FRONTEND_PORT || process.env.VITE_PORT),
      watch: {
        usePolling: true,  // Docker 挂载卷需要轮询监听文件变动
      },
      proxy: {
        '/legal': {
          target: apiTarget,
          changeOrigin: true,
        },
        '/api': {
          target: apiTarget,
          changeOrigin: true,
        },
        '/v1': {
          target: apiTarget,
          changeOrigin: true,
        },
        '/v2': {
          target: apiTarget,
          changeOrigin: true,
        },
        '/v1beta': {
          target: apiTarget,
          changeOrigin: true,
        },
        '/anthropic': {
          target: apiTarget,
          changeOrigin: true,
        },
        '/compatible-mode': {
          target: apiTarget,
          changeOrigin: true,
        },
        '/compatible-api': {
          target: apiTarget,
          changeOrigin: true,
        },
        '/assets/icons/': {
          target: apiTarget,
          changeOrigin: true,
        },
        '/text-to-video': {
          target: apiTarget,
          changeOrigin: true,
        },
        '/image-to-video': {
          target: apiTarget,
          changeOrigin: true,
        },
        '/omni-video': {
          target: apiTarget,
          changeOrigin: true,
        },
        '/tasks': {
          target: apiTarget,
          changeOrigin: true,
        },
        '/ent': {
          target: apiTarget,
          changeOrigin: true,
        },
        // /home/models 由 React 模型广场页面处理，其余 /home 门户页面代理到后端
        '^/home(?:[?#].*|/(?!models(?:[/?#]|$)).*|/?$)': {
          target: apiTarget,
          changeOrigin: true,
        },
        // 站点门户增强版公开页（/home-pro/devdocs 与 /home-pro/blog、旧 /home-pro/docs、/home-pro/guide 由前端 React 接管）
        '^/home-pro(?:[?#].*|/(?!docs(?:[/?#]|$)|guide(?:[/?#]|$)|devdocs(?:[/?#]|$)|blog(?:[/?#]|$)).*|/?$)': {
          target: apiTarget,
          changeOrigin: true,
        },
        // 本地开发不接管 / ，避免挡住控制台。关于、联系、站点地图走后端根路径渲染。
        '^/about(?:[?#].*|/?$)': {
          target: apiTarget,
          changeOrigin: true,
          rewrite: (path: string) => path.replace(/^\/about/, '/portal-about'),
        },
        '^/contact(?:[?#].*|/?$)': {
          target: apiTarget,
          changeOrigin: true,
          rewrite: (path: string) => path.replace(/^\/contact/, '/portal-contact'),
        },
        // 门户静态页的其它语言版本：/en/、/en/about、/en/contact
        '^/(?:zh|zh-TW|en|ja|ko|vi)(?:/|/about/?|/contact/?)?(?:\\?|$)': {
          target: apiTarget,
          changeOrigin: true,
          rewrite: (path: string) => `/portal-lang${path}`,
        },
        '^/sitemap\\.xml$': {
          target: apiTarget,
          changeOrigin: true,
          rewrite: () => '/portal-sitemap.xml',
        },
        '^/robots\\.txt$': {
          target: apiTarget,
          changeOrigin: true,
          rewrite: () => '/portal-robots.txt',
        },
        '/portal-root': {
          target: apiTarget,
          changeOrigin: true,
        },
        '/portal-published': {
          target: apiTarget,
          changeOrigin: true,
        },
        '^/devdocs(?:/|$)': {
          target: apiTarget,
          changeOrigin: true,
          rewrite: (path: string) => path.replace(/^\/devdocs/, '/portal-devdocs'),
        },
        '/portal-pro': {
          target: apiTarget,
          changeOrigin: true,
        },
        '/portal': {
          target: apiTarget,
          changeOrigin: true,
        }
      }
    },
    optimizeDeps: {
      include: [
        'react',
        'react-dom',
        'react-dom/client',
        'react-router-dom',
        'react-is',
        'use-sync-external-store',
        'use-sync-external-store/shim',
        'use-sync-external-store/shim/with-selector',
        'antd',
        '@ant-design/icons',
        'dayjs',
        'zustand',
        'axios',
        'lucide-react',
        'recharts',
        'i18next',
        'react-i18next',
        'react-resizable',
        '@radix-ui/react-popover',
        '@radix-ui/react-dialog',
        '@radix-ui/react-dropdown-menu',
        '@radix-ui/react-select',
        '@radix-ui/react-slider',
        '@radix-ui/react-switch',
        '@radix-ui/react-tooltip',
        '@radix-ui/react-avatar',
        '@radix-ui/react-checkbox',
        'three',
        '@react-three/fiber',
        'html2canvas',
        'jspdf',
        'artplayer',
      ],
    },
    build: {
      assetsDir: 'static', // 避免默认的 assets 目录与前端路由 /assets 冲突，导致 Nginx 报 403
      chunkSizeWarningLimit: 1200,
      rollupOptions: {
        output: {
          manualChunks(id) {
            if (id.includes('node_modules')) {
              if (id.includes('three') || id.includes('@react-three')) {
                return 'three-vendor';
              }
              if (id.includes('recharts') || id.includes('d3-')) {
                return 'recharts-vendor';
              }
              if (id.includes('highlight.js') || id.includes('rehype') || id.includes('remark')) {
                return 'markdown-vendor';
              }
            }
          },
        },
      },
    }
  }
})

