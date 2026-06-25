use serde::{Deserialize, Serialize};

use crate::models;

#[derive(Serialize, Deserialize, Clone)]
pub struct FileImportResult {
    pub imported: Vec<models::Account>,
    pub failed: Vec<FileImportFailure>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct FileImportFailure {
    pub email: String,
    pub error: String,
}
