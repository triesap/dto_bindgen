use dto_bindgen::Dto;

#[allow(dead_code)]
#[derive(Dto)]
#[serde(rename_all = "camelCase")]
struct PostalAddress {
    line_1: String,
}

#[allow(dead_code)]
#[derive(Dto)]
#[serde(rename_all = "camelCase")]
enum UserRole {
    Admin,
    GuestUser,
}

#[allow(dead_code)]
#[derive(Dto)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct UserProfile {
    user_id: String,
    active: bool,
    address: PostalAddress,
    role: UserRole,
}

#[allow(dead_code)]
#[derive(Dto)]
#[serde(
    tag = "type",
    content = "payload",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
enum SdkEvent {
    UserCreated { user: UserProfile, event_id: String },
}

#[allow(dead_code)]
#[derive(Dto)]
struct LedgerEntry {
    #[dto(int = "json_string")]
    amount_minor_units: u128,
}

fn main() -> Result<(), dto_bindgen::export::ExportError> {
    dto_bindgen::export_types!(
        config = "dto_bindgen.toml",
        roots = [UserProfile, UserRole, SdkEvent, LedgerEntry],
    )?;
    Ok(())
}
