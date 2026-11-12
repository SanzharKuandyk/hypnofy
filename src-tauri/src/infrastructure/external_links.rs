//! Opens public model documentation with the system browser, without a shell.

use crate::domain::{ApiResult, AppError};

pub fn model_source(value: &str) -> ApiResult<reqwest::Url> {
    let url =
        reqwest::Url::parse(value).map_err(|_| AppError::from("Invalid model source link"))?;
    if url.scheme() != "https"
        || url.host_str() != Some("huggingface.co")
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port_or_known_default() != Some(443)
    {
        return Err("Model links must use the public Hugging Face website".into());
    }
    Ok(url)
}

#[cfg(windows)]
pub fn open_model_source(value: &str) -> ApiResult<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::UI::{Shell::ShellExecuteW, WindowsAndMessaging::SW_SHOWNORMAL};

    let url = model_source(value)?;
    let encoded = std::ffi::OsStr::new(url.as_str())
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let operation = "open\0".encode_utf16().collect::<Vec<_>>();
    // SAFETY: both buffers are NUL terminated and live through this synchronous call.
    // No parameters, working directory, or executable path come from model metadata.
    let result = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            operation.as_ptr(),
            encoded.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            SW_SHOWNORMAL,
        )
    };
    if result as isize <= 32 {
        return Err("The default browser could not open this model source".into());
    }
    Ok(())
}

#[cfg(not(windows))]
pub fn open_model_source(value: &str) -> ApiResult<()> {
    model_source(value)?;
    Err("Opening model sources is currently supported on Windows".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_links_cannot_execute_commands_or_substitute_another_origin() {
        assert!(model_source("https://huggingface.co/Qwen/Qwen3-TTS/blob/main/README.md").is_ok());
        for link in [
            "file:///C:/Windows/notepad.exe",
            "javascript:alert(1)",
            "https://huggingface.co.evil.test/model",
            "https://user:secret@huggingface.co/model",
            "https://huggingface.co:444/model",
        ] {
            assert!(model_source(link).is_err());
        }
    }
}
