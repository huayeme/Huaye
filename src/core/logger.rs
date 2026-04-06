use crate::core::events::{AppEvent, GLOBAL_EVENT_TX};
use flume::Sender;
use std::env;
use std::fs;
use std::path::PathBuf;
use tracing_subscriber::fmt::writer::MakeWriterExt;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

/// 初始化纯绿色本地日志系统与全局崩溃遥测
/// 返回的 WorkerGuard 必须在 main 函数中持有，否则非阻塞日志写入器会被提前关闭
pub fn init(event_tx: Sender<AppEvent>) -> tracing_appender::non_blocking::WorkerGuard {
    // 尽早初始化全局事件发送器，确保任何阶段 panic 都能发送 FatalError
    let _ = GLOBAL_EVENT_TX.get_or_init(|| event_tx);

    let exe_dir = if let Ok(exe_path) = env::current_exe() {
        exe_path
            .parent()
            .unwrap_or(std::path::Path::new("."))
            .to_path_buf()
    } else {
        env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
    };

    let log_dir = exe_dir.join("logs");
    if !log_dir.exists() {
        let _ = fs::create_dir_all(&log_dir);
    }

    // 清理超过 7 天的旧日志文件
    cleanup_old_logs(&log_dir, 7);

    let now = chrono::Local::now();
    let pid = std::process::id();
    let log_filename = format!("huaye_{}_{}.log", now.format("%Y%m%d_%H%M%S"), pid);

    let file_appender = tracing_appender::rolling::never(log_dir, log_filename);
    let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);

    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    let subscriber = tracing_subscriber::registry().with(env_filter).with(
        tracing_subscriber::fmt::layer()
            .with_writer(non_blocking.and(std::io::stdout))
            .with_ansi(false)
            .with_thread_ids(true)
            .with_target(true)
            .with_file(true)
            .with_line_number(true),
    );

    let _ = subscriber.try_init();

    std::panic::set_hook(Box::new(|panic_info| {
        let backtrace = std::backtrace::Backtrace::force_capture();
        let msg = match panic_info.payload().downcast_ref::<&'static str>() {
            Some(s) => *s,
            None => match panic_info.payload().downcast_ref::<String>() {
                Some(s) => &s[..],
                None => "Box<dyn Any>",
            },
        };

        let location_str = if let Some(loc) = panic_info.location() {
            format!("{}:{}:{}", loc.file(), loc.line(), loc.column())
        } else {
            "<unknown>".to_string()
        };
        let error_msg = format!(
            "致命崩溃 (Panic) 发生在 {}\n原因: {}\n\n堆栈跟踪:\n{}",
            location_str, msg, backtrace
        );

        tracing::error!("{}", error_msg);

        if let Some(tx) = GLOBAL_EVENT_TX.get() {
            let _ = tx.try_send(AppEvent::FatalError(error_msg));
        }
    }));

    tracing::info!("系统日志初始化完成，应用已启动");

    _guard
}

fn cleanup_old_logs(log_dir: &std::path::Path, max_age_days: u64) {
    let cutoff = match std::time::SystemTime::now()
        .checked_sub(std::time::Duration::from_secs(max_age_days * 24 * 3600))
    {
        Some(t) => t,
        None => return, // 系统时钟异常，跳过清理
    };

    let entries = match fs::read_dir(log_dir) {
        Ok(entries) => entries,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("log") {
            continue;
        }
        if let Ok(meta) = path.metadata() {
            let modified = meta.modified().unwrap_or(std::time::SystemTime::UNIX_EPOCH);
            if modified < cutoff {
                let _ = fs::remove_file(&path);
            }
        }
    }
}

#[cfg(test)]
mod tests {}
