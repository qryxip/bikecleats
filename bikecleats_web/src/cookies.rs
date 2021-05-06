use anyhow::{anyhow, Context as _};
use fs2::FileExt as _;
use itertools::Itertools as _;
use reqwest::header::HeaderValue;
use std::{
    fs::File,
    io::{BufReader, Seek as _, SeekFrom},
    path::{Path, PathBuf},
    sync::Mutex,
};
use url::Url;

pub struct CookieStorage {
    store: Mutex<cookie_store::CookieStore>,
    file: LazyLockedFile,
}

impl CookieStorage {
    pub fn new(jsonl_path: &Path) -> anyhow::Result<Self> {
        let store = Mutex::new(if jsonl_path.exists() {
            File::open(jsonl_path)
                .map_err(anyhow::Error::from)
                .and_then(|h| {
                    cookie_store::CookieStore::load_json(BufReader::new(h))
                        .map_err(|e| anyhow!("{}", e))
                })
                .with_context(|| {
                    format!("could not load cookies from `{}`", jsonl_path.display())
                })?
        } else {
            cookie_store::CookieStore::default()
        });

        Ok(Self {
            store,
            file: LazyLockedFile::new(jsonl_path),
        })
    }
}

impl reqwest::cookie::CookieStore for CookieStorage {
    fn set_cookies(&self, cookie_headers: &mut dyn Iterator<Item = &HeaderValue>, url: &Url) {
        let cookies = cookie_headers.flat_map(|v| v.to_str().ok()?.parse().ok());
        let mut store = self.store.lock().unwrap();
        store.store_response_cookies(cookies, url);
        self.file
            .overwrite(|file| {
                store.save_json(file).map_err(|e| anyhow!("{}", e))?;
                Ok(())
            })
            .unwrap_or_else(|e| panic!("{}", e))
    }

    fn cookies(&self, url: &Url) -> Option<HeaderValue> {
        let header = self
            .store
            .lock()
            .unwrap()
            .get_request_cookies(url)
            .join("; ");
        (!header.is_empty()).then(|| ())?;
        header.parse().ok()
    }
}

#[derive(Debug)]
struct LazyLockedFile {
    path: PathBuf,
    file: Mutex<Option<File>>,
}

impl LazyLockedFile {
    fn new(path: &Path) -> Self {
        Self {
            path: path.to_owned(),
            file: Mutex::new(None),
        }
    }

    fn overwrite(&self, f: impl FnOnce(&mut File) -> anyhow::Result<()>) -> anyhow::Result<()> {
        let Self { path, file } = self;

        let mut file = file.lock().unwrap();

        let new_file = if file.is_none() {
            if let Some(parent) = path.parent() {
                if !parent.exists() {
                    std::fs::create_dir_all(parent)
                        .with_context(|| format!("could not create `{}`", parent.display()))?;
                }
            }

            let new_file = File::create(&path)
                .with_context(|| format!("could not open `{}`", path.display()))?;

            new_file
                .try_lock_exclusive()
                .with_context(|| format!("could not lock `{}`", path.display()))?;

            Some(new_file)
        } else {
            None
        };

        let file = file.get_or_insert_with(|| new_file.unwrap());

        file.seek(SeekFrom::Start(0))
            .and_then(|_| file.set_len(0))
            .map_err(Into::into)
            .and_then(|()| f(file))
            .and_then(|()| file.sync_data().map_err(Into::into))
            .with_context(|| format!("could not write `{}`", path.display()))
    }
}
