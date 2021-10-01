use std::mem;
use std::fs;
use std::io;
use std::env;
use std::path::{ Path, PathBuf};

use s3_server::S3Service;
use s3_server::storages::fs::FileSystem;
use s3_server::path::S3Path;

use anyhow::{anyhow, Result};
use tracing::{debug_span, error};
pub type Request = hyper::Request<hyper::Body>;
pub type Response = hyper::Response<hyper::Body>;

pub trait ResultExt<T, E> {
    fn inspect_err(self, f: impl FnOnce(&mut E)) -> Self;
}

impl<T, E> ResultExt<T, E> for Result<T, E> {
    fn inspect_err(mut self, f: impl FnOnce(&mut E)) -> Self {
        if let Err(ref mut e) = self {
            f(e)
        }
        self
    }
}

macro_rules! enter_sync {
    ($span:expr) => {
        let __span = $span;
        let __enter = __span.enter();
    };
}

pub async fn recv_body_string(res: &mut Response) -> Result<String> {
    let body = mem::take(res.body_mut());
    let bytes = hyper::body::to_bytes(body).await?;
    let ans = String::from_utf8(bytes.to_vec())?;
    Ok(ans)
}

pub fn generate_path(root: impl AsRef<Path>, path: S3Path) -> PathBuf {
    match path {
        S3Path::Root => root.as_ref().to_owned(),
        S3Path::Bucket { bucket } => root.as_ref().join(bucket),
        S3Path::Object { bucket, key } => root.as_ref().join(bucket).join(key),
    }
}

pub fn setup_tracing() {
    use tracing_error::ErrorLayer;
    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::util::SubscriberInitExt;
    use tracing_subscriber::{fmt, EnvFilter};

    tracing_subscriber::fmt()
        .event_format(fmt::format::Format::default().pretty())
        .with_env_filter(EnvFilter::from_default_env())
        .with_timer(fmt::time::ChronoLocal::rfc3339())
        .finish()
        .with(ErrorLayer::default())
        .try_init()
        .ok();
}

pub fn setup_fs_root(clear: bool) -> Result<PathBuf> {
    let root: PathBuf = env::var("S3_TEST_FS_ROOT")
        .unwrap_or_else(|_| "target/s3-test".into())
        .into();

    enter_sync!(debug_span!("setup fs root", ?clear, root = %root.display()));

    let exists = root.exists();
    if exists && clear {
        fs::remove_dir_all(&root)
            .inspect_err(|err| error!(%err,"failed to remove root directory"))?;
    }

    if !exists || clear {
        fs::create_dir_all(&root).inspect_err(|err| error!(%err, "failed to create directory"))?;
    }

    if !root.exists() {
        let err = anyhow!("root does not exist");
        error!(%err);
        return Err(err);
    }

    Ok(root)
}

pub fn setup_service() -> Result<(PathBuf, S3Service)> {
    setup_tracing();

    let root = setup_fs_root(true).unwrap();

    enter_sync!(debug_span!("setup service", root = %root.display()));

    let fs =
        FileSystem::new(&root).inspect_err(|err| error!(%err, "failed to create filesystem"))?;

    let service = S3Service::new(fs);

    Ok((root, service))
}

#[tracing::instrument(
    skip(root),
    fields(root = %root.as_ref().display()),
)]
pub fn fs_write_object(
    root: impl AsRef<Path>,
    bucket: &str,
    key: &str,
    content: &str,
) -> io::Result<()> {
    let dir_path = generate_path(&root, S3Path::Bucket { bucket });
    if !dir_path.exists() {
        fs::create_dir(dir_path)?;
    }
    let file_path = generate_path(root, S3Path::Object { bucket, key });
    fs::write(file_path, content)
}