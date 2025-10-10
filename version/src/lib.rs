#![allow(non_upper_case_globals)]

pub static version: &str = concat!("r", env!("VERGEN_GIT_COMMIT_COUNT"), ".", env!("VERGEN_GIT_SHA"));
