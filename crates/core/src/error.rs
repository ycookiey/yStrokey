use std::fmt;

/// アプリケーションエラーの統合型
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    /// Win32 APIエラー
    #[error("Win32 error: {0}")]
    Win32(String),

    /// 描画エラー
    #[error(transparent)]
    Render(#[from] RenderError),

    /// 設定ファイルエラー
    #[error(transparent)]
    Config(#[from] ConfigError),

    /// 入力フックエラー
    #[error(transparent)]
    Hook(#[from] HookError),
}

#[derive(Debug, thiserror::Error)]
pub enum RenderError {
    #[error("device lost")]
    DeviceLost,

    #[error("create failed: {0}")]
    CreateFailed(String),

    #[error("draw failed: {0}")]
    DrawFailed(String),
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("parse error: {0}")]
    ParseError(#[from] serde_json::Error),

    #[error("validation error: {0}")]
    ValidationError(String),
}

#[derive(Debug)]
pub enum HookError {
    SetHookFailed(String),
    MessageLoopFailed,
}

impl fmt::Display for HookError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HookError::SetHookFailed(msg) => write!(f, "set hook failed: {}", msg),
            HookError::MessageLoopFailed => write!(f, "message loop failed"),
        }
    }
}

impl std::error::Error for HookError {}
