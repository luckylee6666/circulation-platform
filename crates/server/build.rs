//! 前端产物由 rust-embed 在编译期内嵌进二进制。
//!
//! 桌面端与独立运行都靠它提供网页，因此打包后不需要额外分发静态文件。
//! 前端尚未构建时（例如只跑后端测试），这里会先放一个占位页，保证后端可以独立编译。

use std::path::PathBuf;

const PLACEHOLDER: &str = r#"<!doctype html>
<html lang="zh-CN">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>流转平台</title>
<style>
  body { margin: 0; min-height: 100vh; display: grid; place-items: center;
         font-family: -apple-system, "Segoe UI", "Microsoft YaHei", sans-serif;
         background: #f5f7fa; color: #1f2937; }
  .card { text-align: center; padding: 48px 56px; background: #fff; border-radius: 16px;
          box-shadow: 0 8px 32px rgba(15, 23, 42, .08); }
  h1 { margin: 0 0 8px; font-size: 20px; }
  p { margin: 0; color: #6b7280; font-size: 14px; line-height: 1.7; }
  code { background: #f1f5f9; padding: 2px 6px; border-radius: 4px; font-size: 13px; }
</style>
</head>
<body>
  <div class="card">
    <h1>流转平台服务已就绪</h1>
    <p>接口服务运行正常，前端界面尚未构建。<br>
       请在项目根目录执行 <code>npm run build:web</code> 后重新启动。</p>
  </div>
</body>
</html>
"#;

fn main() {
    let dist = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../apps/web/dist");

    if !dist.join("index.html").exists() {
        if let Err(err) = std::fs::create_dir_all(&dist) {
            panic!("无法创建前端产物目录 {}: {err}", dist.display());
        }
        if let Err(err) = std::fs::write(dist.join("index.html"), PLACEHOLDER) {
            panic!("无法写入占位页: {err}");
        }
    }

    println!("cargo:rerun-if-changed=../../apps/web/dist");
}
