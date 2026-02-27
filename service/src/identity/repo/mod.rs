//! Repository layer for identity persistence

pub mod accounts;
pub mod backups;
pub mod device_keys;

pub use accounts::{create_account, AccountRepoError, CreatedAccount};
pub use backups::{
    create_backup, delete_backup_by_kid, get_backup_by_kid, BackupRecord, BackupRepoError,
    CreatedBackup,
};
pub use device_keys::{
    create_device_key_with_conn, get_device_key_by_kid, list_device_keys_by_account,
    rename_device_key, revoke_device_key, touch_device_key, CreatedDeviceKey, DeviceKeyRecord,
    DeviceKeyRepoError,
};
