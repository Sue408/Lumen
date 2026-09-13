use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};

use chrono::Local;
use tracing_subscriber::fmt::writer::Tee;
use tracing_subscriber::fmt::MakeWriter;

/// 应用运行日志的目录名（相对 `app_data_dir`）。
pub const LOG_DIR: &str = "logs";

/// 按天滚动的日志写入器。
///
/// release 构建在 Windows 上是 `windows_subsystem = "windows"`，没有控制台，`tracing`
/// 默认的 stdout 输出会被直接丢弃——这正是「网关应用自身无日志」的根因。这里把日志
/// 落到 `app_data_dir/logs/lumen-YYYY-MM-DD.log`，跨天首次写入时自动切到新文件。
#[derive(Clone)]
pub struct DailyFileWriter {
    dir: Arc<PathBuf>,
    state: Arc<Mutex<WriterState>>,
}

struct WriterState {
    date: String,
    file: File,
}

impl DailyFileWriter {
    pub fn new(dir: PathBuf) -> io::Result<Self> {
        std::fs::create_dir_all(&dir)?;
        let date = today();
        let file = open_file(&dir, &date)?;
        Ok(Self {
            dir: Arc::new(dir),
            state: Arc::new(Mutex::new(WriterState { date, file })),
        })
    }

    /// 每次格式化日志前调用：日期变了就换文件。沿用旧文件句柄会导致日志一直写进
    /// 启动那天的文件，长期运行的网关跨天也不轮转。
    fn make_writer_locked(&self) -> DailyFileGuard<'_> {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        let date = today();
        if state.date != date {
            if let Ok(file) = open_file(&self.dir, &date) {
                state.file = file;
                state.date = date;
            }
        }
        DailyFileGuard { state }
    }
}

fn today() -> String {
    Local::now().format("%Y-%m-%d").to_string()
}

fn open_file(dir: &Path, date: &str) -> io::Result<File> {
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join(format!("lumen-{date}.log")))
}

pub struct DailyFileGuard<'a> {
    state: MutexGuard<'a, WriterState>,
}

impl<'a> MakeWriter<'a> for DailyFileWriter {
    type Writer = DailyFileGuard<'a>;

    fn make_writer(&'a self) -> Self::Writer {
        self.make_writer_locked()
    }
}

impl Write for DailyFileGuard<'_> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.state.file.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.state.file.flush()
    }
}

/// 初始化全局日志：始终写文件；debug 构建额外镜像到进程 stdout，方便 `tauri dev`
/// 在终端直接看日志。重复初始化（如测试）安全跳过。
pub fn init(app_data_dir: &Path) {
    let dir = app_data_dir.join(LOG_DIR);
    match DailyFileWriter::new(dir) {
        Ok(file) => init_with(file),
        Err(error) => {
            // 文件日志都开不起来时保留 stdout，至少 Runtime 日志还可诊断。
            eprintln!("运行日志初始化失败，退回标准输出：{error}");
            let _ = tracing_subscriber::fmt()
                .with_max_level(tracing::Level::INFO)
                .with_target(false)
                .with_ansi(false)
                .try_init();
        }
    }
}

fn init_with(file: DailyFileWriter) {
    let builder = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .with_target(false)
        .with_ansi(false);

    #[cfg(debug_assertions)]
    let builder = builder.with_writer(Tee::new(std::io::stdout, file));
    #[cfg(not(debug_assertions))]
    let builder = builder.with_writer(file);

    let _ = builder.try_init();

    // panic 会带着 abort 结束进程（release profile 设了 panic = "abort"），所以在钩子
    // 里先落一条 error，别让崩溃现场无声消失。
    std::panic::set_hook(Box::new(|info| {
        tracing::error!("应用发生 panic：{info}");
    }));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex as StdMutex;

    static LOCK: StdMutex<()> = StdMutex::new(());

    #[test]
    fn writes_events_into_the_daily_file() {
        let _guard = LOCK.lock().unwrap_or_else(|poison| poison.into_inner());
        let dir = std::env::temp_dir().join(format!("lumen-log-{}", uuid::Uuid::new_v4()));

        {
            let file = DailyFileWriter::new(dir.clone()).unwrap();
            let subscriber = tracing_subscriber::fmt()
                .with_max_level(tracing::Level::INFO)
                .with_target(false)
                .with_ansi(false)
                .with_writer(file)
                .finish();
            tracing::subscriber::with_default(subscriber, || {
                tracing::info!("探测器已就绪");
            });
        }

        let mut entries: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .collect();
        entries.sort();
        assert_eq!(entries.len(), 1, "应恰好生成一个当日日志文件");
        let content = std::fs::read_to_string(&entries[0]).unwrap();
        assert!(content.contains("探测器已就绪"), "日志内容：{content}");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn appends_within_the_same_day_file() {
        let _guard = LOCK.lock().unwrap_or_else(|poison| poison.into_inner());
        let dir = std::env::temp_dir().join(format!("lumen-log-{}", uuid::Uuid::new_v4()));

        {
            let file = DailyFileWriter::new(dir.clone()).unwrap();
            let subscriber = tracing_subscriber::fmt()
                .with_target(false)
                .with_ansi(false)
                .with_writer(file)
                .finish();
            tracing::subscriber::with_default(subscriber, || {
                tracing::info!("第一行");
                tracing::info!("第二行");
            });
        }

        let entry = std::fs::read_dir(&dir).unwrap().next().unwrap().unwrap();
        let content = std::fs::read_to_string(entry.path()).unwrap();
        assert!(content.contains("第一行") && content.contains("第二行"));

        std::fs::remove_dir_all(&dir).ok();
    }
}
