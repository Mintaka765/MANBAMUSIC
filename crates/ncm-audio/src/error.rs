/// 音频层错误。
#[derive(Debug, thiserror::Error)]
pub enum AudioError {
    #[error("直链格式非法: {0}")]
    BadUrl(String),

    #[error("建立音频流失败: {0}")]
    Stream(String),

    #[error("解码失败: {0}")]
    Decode(#[from] rodio::decoder::DecoderError),

    #[error("没有可用的音频输出设备: {0}")]
    Device(String),
}
