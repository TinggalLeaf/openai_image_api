import type { Metadata } from "next";
import "./globals.css";
import { AuthProvider } from "@/components/auth-provider";
import { HealthProvider } from "@/components/health-provider";
import { ThemeProvider } from "@/components/theme-provider";

export const metadata: Metadata = {
  title: "图像生成网关 · 管理面板",
  description: "OpenAI 兼容图像生成网关的管理控制台",
};

/**
 * Tiny inline script that runs before paint to set the persisted
 * theme on <html>. Without this, a returning user with theme=light
 * sees a brief flash of the dark default.
 */
const themeBootScript = `(function(){try{var t=localStorage.getItem('admin_theme');if(t==='light'){document.documentElement.classList.remove('dark');}else{document.documentElement.classList.add('dark');}document.documentElement.dataset.theme=t==='light'?'light':'dark';}catch(e){}})();`;

export default function RootLayout({ children }: LayoutProps<"/">) {
  return (
    <html lang="zh-CN" className="dark h-full">
      <head>
        <script dangerouslySetInnerHTML={{ __html: themeBootScript }} />
      </head>
      <body className="min-h-full bg-[var(--background)] text-[var(--foreground)]">
        <ThemeProvider>
          <AuthProvider>
            <HealthProvider>{children}</HealthProvider>
          </AuthProvider>
        </ThemeProvider>
      </body>
    </html>
  );
}
