use std::path::Path;

use stream_download::storage::temp::TempStorageProvider;
use stream_download::{Settings, StreamDownload};
use url::Url;

use crate::AudioError;

/// 打开一条边下边播的流。
///
/// 网易云直链支持 HTTP Range（实测 206），所以 `StreamDownload` 的随机 Seek 可用；
/// 创建后后台任务会立刻开始下载，等于自带预取。
///
/// `cache_dir` 决定流缓冲落在哪：传应用自己的缓存目录，别依赖系统临时目录
/// （受限环境里 %TEMP% 可能写不进去）。
pub async fn open(
    url: &str,
    cache_dir: &Path,
) -> Result<StreamDownload<TempStorageProvider>, AudioError> {
    let parsed = Url::parse(url).map_err(|err| AudioError::BadUrl(err.to_string()))?;

    std::fs::create_dir_all(cache_dir).map_err(|err| AudioError::Stream(err.to_string()))?;
    let storage = TempStorageProvider::new_in(cache_dir);

    StreamDownload::new_http(parsed, storage, Settings::default())
        .await
        .map_err(|err| AudioError::Stream(err.to_string()))
}
