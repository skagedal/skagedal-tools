use std::env;

pub struct ProcessEnvironment;

impl ProcessEnvironment {
    pub fn suggested_cd_file() -> Option<String> {
        env::var("SUGGESTED_CD_FILE").ok()
    }
}
