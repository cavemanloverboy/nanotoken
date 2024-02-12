use solana_program::program_error::ProgramError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum NanoTokenError {
    DuplicateAccount,
    InsufficientTokenBalance,
    InvalidDecimals,
    IncorrectMint,
    SupplyOverflow,
}

impl From<NanoTokenError> for ProgramError {
    fn from(e: NanoTokenError) -> ProgramError {
        ProgramError::Custom(e as u32)
    }
}
