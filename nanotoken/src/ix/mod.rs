use solana_program::program_error::ProgramError;
use strum::EnumDiscriminants;

use crate::utils::split_at_unchecked;

pub mod init_config;

pub use init_config::*;

pub mod initialize_mint;
pub use initialize_mint::*;

pub mod initialize_account;
pub use initialize_account::*;

pub mod mint;
pub use mint::*;

pub mod burn;
pub use burn::*;

pub use transfer::*;
pub mod transfer;

pub mod init_vault;
pub use init_vault::*;

pub use transmute::*;
pub mod transmute;

#[derive(PartialEq, Debug, Clone, Copy, EnumDiscriminants)]
#[strum_discriminants(name(Tag))]
#[repr(u64)]
pub enum ProgramInstruction {
    /// This should run only once at the beginning of the program
    InitializeConfig(InitConfigArgs),

    InitializeMint(InitializeMintArgs),
    InitializeAccount(InitializeAccountArgs),
    InitializeVault(InitializeVaultArgs),
    Mint(MintArgs),
    Burn(BurnArgs),
    Transfer(TransferArgs),
    Transmute(TransmuteArgs),
}

impl Tag {
    pub const fn size() -> usize {
        core::mem::size_of::<Self>()
    }
}

#[repr(u64)]
pub(crate) enum ProgramInstructionRef<'a> {
    InitializeConfig(&'a InitConfigArgs),
    InitializeAccount(&'a InitializeAccountArgs),
    InitializeMint(&'a InitializeMintArgs),
    InitializeVault(&'a InitializeVaultArgs),
    Mint(&'a MintArgs),
    Burn(&'a BurnArgs),
    Transfer(&'a TransferArgs),
    Transmute(&'a TransmuteArgs),
}

pub(crate) struct InstructionIter<'a> {
    data: &'a [u8],
}

impl<'a> InstructionIter<'a> {
    pub fn new(data: &'a [u8]) -> InstructionIter<'a> {
        InstructionIter { data }
    }
}

impl<'a> Iterator for InstructionIter<'a> {
    type Item = Result<ProgramInstructionRef<'a>, ProgramError>;
    fn next(&mut self) -> Option<Self::Item> {
        if self.data.len() < Tag::size() {
            return None;
        }

        // Parse tag
        // SAFETY:
        // We do length check manually to return None instead of panicking
        let (tag, data) = unsafe { split_at_unchecked(self.data, 8) };
        let tag = unsafe { *(tag.as_ptr() as *const u8) }; // little endian
        self.data = data;

        match tag {
            x if x == Tag::InitializeConfig as u8 => Some(
                InitConfigArgs::from_data(&mut self.data)
                    .map(ProgramInstructionRef::InitializeConfig),
            ),

            x if x == Tag::InitializeMint as u8 => Some(
                InitializeMintArgs::from_data(&mut self.data)
                    .map(ProgramInstructionRef::InitializeMint),
            ),

            x if x == Tag::InitializeAccount as u8 => Some(
                InitializeAccountArgs::from_data(&mut self.data)
                    .map(ProgramInstructionRef::InitializeAccount),
            ),

            x if x == Tag::InitializeVault as u8 => Some(
                InitializeVaultArgs::from_data(&mut self.data)
                    .map(ProgramInstructionRef::InitializeVault),
            ),

            x if x == Tag::Mint as u8 => Some(
                MintArgs::from_data(&mut self.data)
                    .map(ProgramInstructionRef::Mint),
            ),

            x if x == Tag::Burn as u8 => Some(
                BurnArgs::from_data(&mut self.data)
                    .map(ProgramInstructionRef::Burn),
            ),

            x if x == Tag::Transfer as u8 => Some(
                TransferArgs::from_data(&mut self.data)
                    .map(ProgramInstructionRef::Transfer),
            ),

            x if x == Tag::Transmute as u8 => Some(
                TransmuteArgs::from_data(&mut self.data)
                    .map(ProgramInstructionRef::Transmute),
            ),

            _ => None,
        }
    }
}
