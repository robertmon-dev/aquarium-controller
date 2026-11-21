#[cfg(target_os = "espidf")]
pub use hw::{read_blob, replace_blob};

#[cfg(target_os = "espidf")]
mod hw {
    use std::ffi::CString;

    use esp_idf_svc::handle::RawHandle;
    use esp_idf_svc::nvs::{EspNvs, NvsDefault};
    use esp_idf_svc::sys::{
        esp, nvs_commit, nvs_set_blob, EspError, ESP_ERR_INVALID_ARG, ESP_ERR_INVALID_SIZE,
    };

    pub fn replace_blob(nvs: &EspNvs<NvsDefault>, key: &str, bytes: &[u8]) -> Result<(), EspError> {
        let key =
            CString::new(key).map_err(|_| EspError::from_infallible::<ESP_ERR_INVALID_ARG>())?;
        let handle = nvs.handle();

        esp!(unsafe { nvs_set_blob(handle, key.as_ptr(), bytes.as_ptr().cast(), bytes.len()) })?;
        esp!(unsafe { nvs_commit(handle) })
    }

    pub fn read_blob(
        nvs: &EspNvs<NvsDefault>,
        key: &str,
        max_bytes: usize,
    ) -> Result<Option<Vec<u8>>, EspError> {
        let Some(length) = nvs.blob_len(key)? else {
            return Ok(None);
        };

        if length > max_bytes {
            return Err(EspError::from_infallible::<ESP_ERR_INVALID_SIZE>());
        }

        let mut buffer = vec![0_u8; length];

        Ok(nvs.get_blob(key, &mut buffer)?.map(<[u8]>::to_vec))
    }
}
