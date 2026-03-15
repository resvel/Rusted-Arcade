mod db;

pub use db::{
    DataError, Database, GamepadMappingRecord, ScanUpsertOutcome, ScannedRomInput,
    VersionConflictError, CLOUD_SLOT_COUNT, CLOUD_SLOT_MAX, CLOUD_SLOT_MIN,
};
