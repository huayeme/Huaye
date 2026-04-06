fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").unwrap() == "windows" {
        let mut res = winres::WindowsResource::new();
        // 设置 EXE 文件图标
        res.set_icon("src/static/app.ico");
        res.compile().unwrap();
    }
}
