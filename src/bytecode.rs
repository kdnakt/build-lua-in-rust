#[derive(Debug)]
pub enum ByteCode {
    GetGlobal(u8, u8),
    LoadConst(u8, u8),
    LoadNil(u8),
    LoadBool(u8, bool),
    Call(u8, u8),
}
