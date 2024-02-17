#![no_std]

use bytemuck::{Pod, Zeroable};
use consts::CONFIG_ACCOUNT;
use solana_nostd_entrypoint::{
    entrypoint_nostd,
    solana_program::{
        self, declare_id, entrypoint::ProgramResult,
        program_error::ProgramError, pubkey::Pubkey,
        system_program::ID as SYSTEM_PROGRAM,
    },
    NoStdAccountInfo4,
};

pub mod ix;
use ix::{ProgramInstructionRef as Ix, *};
use solana_program::log;
pub mod consts;
pub(crate) mod utils;

pub mod error;

declare_id!("GigabithNd6HmU4nRFPHXAkBK9nAtvNuHnSavWi3G7Zj");

#[cfg(not(feature = "no-entrypoint"))]
entrypoint_nostd!(process_instruction_nostd, 64);

pub mod allocator {
    pub struct NoAlloc;
    extern crate alloc;
    unsafe impl alloc::alloc::GlobalAlloc for NoAlloc {
        #[inline]
        unsafe fn alloc(&self, _: core::alloc::Layout) -> *mut u8 {
            panic!("no_alloc :)");
        }
        #[inline]
        unsafe fn dealloc(&self, _: *mut u8, _: core::alloc::Layout) {}
    }

    #[cfg(target_os = "solana")]
    #[global_allocator]
    static A: NoAlloc = NoAlloc;
}

fn process_instruction_nostd(
    _program_id: &Pubkey,
    accounts: &[NoStdAccountInfo4],
    data: &[u8],
) -> ProgramResult {
    // We lazily check 2/3 of last 3 here since they may be needed
    // in the proceeding instructions.
    // This memoization makes the validation only happen once.
    // The payer will be checked by any system_program cpis that need to be
    // performed.
    //
    // Every instruction requires at least 3 accounts, so in the case no
    // instruction requires validation then these accounts will not correspond
    // to their labels (which is okay).
    let [_rem @ .., config, system_program, _payer] = accounts else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };

    // Check config
    let mut validated_config = false;
    let mut config_validator = {
        #[inline(always)]
        || {
            if !validated_config {
                if solana_program::program_memory::sol_memcmp(
                    config.key().as_ref(),
                    &CONFIG_ACCOUNT.as_ref(),
                    32,
                ) != 0
                {
                    log::sol_log("config does not have expected pubkey");
                    return Err(ProgramError::InvalidArgument);
                }
                validated_config = true;
            }
            Ok(true)
        }
    };

    // Check system_program
    let mut validated_sys_program = false;
    let mut sys_program_validator = {
        #[inline(always)]
        || {
            if !validated_sys_program {
                if solana_program::program_memory::sol_memcmp(
                    system_program.key().as_ref(),
                    SYSTEM_PROGRAM.as_ref(),
                    32,
                ) != 0
                {
                    log::sol_log(
                        "system_program does not have expected pubkey",
                    );
                    return Err(ProgramError::InvalidArgument);
                }
                validated_sys_program = true;
            }
            Ok(true)
        }
    };

    // Parse program instructions
    let instruction_iter = InstructionIter::new(data);

    let mut ai = 0;
    for instruction in instruction_iter {
        // This will never be oob
        let ix_accounts = unsafe { accounts.get_unchecked(ai..) };

        ai += match instruction? {
            Ix::InitializeConfig(args) => {
                config_validator()?;
                initialize_config(ix_accounts, args)
            }
            Ix::InitializeMint(args) => {
                sys_program_validator()?;
                initialize_mint(ix_accounts, args)
            }
            Ix::InitializeAccount(args) => {
                config_validator()?;
                sys_program_validator()?;
                initialize_account(ix_accounts, args)
            }
            Ix::InitializeVault(args) => {
                config_validator()?;
                sys_program_validator()?;
                initialize_vault(ix_accounts, args)
            }
            Ix::Mint(args) => {
                // don't need to validate config or sys program
                mint(ix_accounts, args)
            }
            Ix::Burn(args) => {
                // don't need to validate config or sys program
                burn(ix_accounts, args)
            }
            Ix::Transfer(args) => {
                // don't need to validate config or sys program
                transfer(ix_accounts, args)
            }
            Ix::Transmute(args) => {
                config_validator()?;
                sys_program_validator()?;
                transmute(ix_accounts, args)
            }
        }?;
    }

    Ok(())
}

#[derive(Clone, Copy, Pod, Zeroable)]
#[repr(C)]
pub struct ProgramConfig {
    mint_index: u64,
}

#[repr(u64)]
#[derive(Debug, Copy, Clone, PartialEq, Default)]
pub enum EventType {
    #[default]
    Join,
    Leave,
}
unsafe impl bytemuck::Pod for EventType {}
unsafe impl bytemuck::Zeroable for EventType {}

unsafe impl bytemuck::Contiguous for EventType {
    type Int = u64;
    const MIN_VALUE: u64 = EventType::Join as u64;
    const MAX_VALUE: u64 = EventType::Leave as u64;
}

impl ProgramConfig {
    pub const fn space() -> usize {
        8 + core::mem::size_of::<Self>()
    }
    pub const fn size() -> usize {
        core::mem::size_of::<Self>()
    }

    /// SAFETY: unchecked refers to refcell checks, not to discriminator checks.
    /// i.e. memory safety. You must ensure no one else has a view into config's
    /// account data.
    ///
    /// Discriminator check is still performed.
    ///
    /// Owner check is not needed as it was checked on initialization, so it is
    /// checked implicitly by the discriminator check.
    pub(crate) unsafe fn unchecked_load_mut(
        config: &NoStdAccountInfo4,
    ) -> Result<&mut ProgramConfig, ProgramError> {
        // Unpack and split data into discriminator & config
        let config_data = config.unchecked_borrow_mut_data();
        let (disc, config_bytes) = config_data.split_at_mut(8);

        // We only need to check the first byte
        if disc[0] != AccountDiscriminator::Config as u8 {
            log::sol_log("config discriminator is incorrect");
            return Err(ProgramError::InvalidAccountData);
        }

        Ok(&mut *(config_bytes.as_mut_ptr() as *mut ProgramConfig))
    }
}

#[repr(u8)]
pub enum AccountDiscriminator {
    Unintialized = 0,
    Config,
    Mint,
    Token,
    VaultInfo,
}

#[derive(Debug, Clone, PartialEq, PartialOrd, Copy, Pod, Zeroable)]
#[repr(C)]
pub struct Mint {
    pub mint_index: u64,
    /// [0; 32] is used as None
    pub authority: Pubkey,
    pub supply: u64,
    pub decimals: u8,
    pub _padding: [u8; 7],
}

impl Mint {
    pub fn size() -> usize {
        core::mem::size_of::<Mint>()
    }

    pub fn space() -> usize {
        8 + core::mem::size_of::<Mint>()
    }

    /// SAFETY: unchecked refers to refcell checks, not to discriminator checks.
    /// i.e. memory safety. You must ensure no one else has a view into config's
    /// account data.
    ///
    /// Discriminator, owner check is still performed.
    /// (owner check need only be performed when we are not mutating mint)
    pub unsafe fn unchecked_load_mut<const OWNER_CHECK: bool>(
        mint: &NoStdAccountInfo4,
    ) -> Result<&mut Mint, ProgramError> {
        // Unpack and split data into discriminator & mint
        let mint_data = mint.unchecked_borrow_mut_data();
        let (disc, mint_bytes) = mint_data.split_at_mut(8);

        // We only need to check the first byte
        if disc[0] != AccountDiscriminator::Mint as u8 {
            log::sol_log("mint discriminator is incorrect");
            return Err(ProgramError::InvalidAccountData);
        }

        // Check owner (only needs to be done if there is no mutation)
        if OWNER_CHECK {
            Mint::owner_check(mint)?;
        }

        Ok(&mut *(mint_bytes.as_mut_ptr() as *mut Mint))
    }

    #[inline(always)]
    pub fn owner_check(mint: &NoStdAccountInfo4) -> ProgramResult {
        if *mint.owner() != crate::ID {
            log::sol_log("mint account has incorrect owner");
            return Err(ProgramError::InvalidAccountOwner);
        }
        Ok(())
    }

    /// TODO DOCS
    pub fn checked_load_mut(
        mint_data: &mut [u8],
    ) -> Result<&mut Mint, ProgramError> {
        // Unpack and split data into discriminator & mint
        let (disc, mint_bytes) = mint_data.split_at_mut(8);

        // We only need to check the first byte
        if disc[0] != AccountDiscriminator::Mint as u8 {
            log::sol_log("mint discriminator is incorrect");
            return Err(ProgramError::InvalidAccountData);
        }

        Ok(unsafe { &mut *(mint_bytes.as_mut_ptr() as *mut Mint) })
    }
}

#[derive(Debug, Clone, PartialEq, PartialOrd, Copy, Pod, Zeroable)]
#[repr(C)]
pub struct TokenAccount {
    pub owner: Pubkey,
    pub mint: u64,
    pub balance: u64,
}

impl TokenAccount {
    pub fn address(mint: u64, owner: &Pubkey) -> (Pubkey, u8) {
        Pubkey::find_program_address(
            &[owner.as_ref(), mint.to_le_bytes().as_ref()],
            &crate::ID,
        )
    }
    pub fn size() -> usize {
        core::mem::size_of::<Self>()
    }

    pub fn space() -> usize {
        8 + core::mem::size_of::<Self>()
    }

    /// SAFETY: unchecked refers to refcell checks, not to discriminator checks.
    /// i.e. memory safety. You must ensure no one else has a view into config's
    /// account data.
    ///
    /// Discriminator is still performed. This does not do an owner check!
    /// If you call this function you MUST mutate the data to do an implicit
    /// owner check (should be mutated during e.g. mint, transfer)
    pub unsafe fn unchecked_load_mut(
        token_account: &NoStdAccountInfo4,
    ) -> Result<&mut TokenAccount, ProgramError> {
        // Unpack and split data into discriminator & token_account
        let token_account_data = token_account.unchecked_borrow_mut_data();
        let (disc, token_account_bytes) = token_account_data.split_at_mut(8);

        // We only need to check the first byte
        if disc[0] != AccountDiscriminator::Token as u8 {
            log::sol_log("token_account discriminator is incorrect");
            return Err(ProgramError::InvalidAccountData);
        }

        Ok(&mut *(token_account_bytes.as_mut_ptr() as *mut TokenAccount))
    }

    /// Discriminator check. This does not do an owner check!
    /// If you call this function you MUST mutate the data to do an implicit
    /// owner check (should be mutated during e.g. mint, transfer)
    pub fn checked_load_mut<'a>(
        token_account_data: &'a mut [u8],
    ) -> Result<&'a mut TokenAccount, ProgramError> {
        // Unpack and split data into discriminator & token_account
        let (disc, token_account_bytes) = token_account_data.split_at_mut(8);

        // We only need to check the first byte
        if disc[0] != AccountDiscriminator::Token as u8 {
            log::sol_log("token_account discriminator is incorrect");
            return Err(ProgramError::InvalidAccountData);
        }

        Ok(unsafe {
            &mut *(token_account_bytes.as_mut_ptr() as *mut TokenAccount)
        })
    }

    pub unsafe fn check_disc(
        token_account: &NoStdAccountInfo4,
    ) -> Result<(&Pubkey, u64, *mut u64), ProgramError> {
        // Unpack and split data into discriminator &token_account
        let (disc, token_account_bytes) = token_account
            .unchecked_borrow_data()
            .split_at(8);

        // We only need to check the first byte
        if disc[0] != AccountDiscriminator::Token as u8 {
            log::sol_log("token_account discriminator is incorrect");
            return Err(ProgramError::InvalidAccountData);
        }

        let account =
            unsafe { &*(token_account_bytes.as_ptr() as *const TokenAccount) };

        Ok((
            &account.owner,
            account.mint,
            &account.balance as *const u64 as *mut u64,
        ))
    }
}

#[derive(Debug, Clone, PartialEq, PartialOrd, Copy, Pod, Zeroable)]
#[repr(C)]
pub struct VaultInfo {
    tokenkeg_mint: Pubkey,
    tokenkeg_vault: Pubkey,
    nanotoken_mint: Pubkey,
    info_bump: u8,
}

impl VaultInfo {
    pub fn space() -> usize {
        8 + core::mem::size_of::<Self>()
    }

    pub fn info(tokenkeg_mint: &Pubkey) -> (Pubkey, u8) {
        Pubkey::find_program_address(
            &[b"info", tokenkeg_mint.as_ref()],
            &crate::ID,
        )
    }

    pub fn vault(tokenkeg_mint: &Pubkey) -> (Pubkey, u8) {
        Pubkey::find_program_address(
            &[b"vault", tokenkeg_mint.as_ref()],
            &crate::ID,
        )
    }

    /// Discriminator and owner checks are performed.
    pub(crate) fn checked_load<'a>(
        vault_info_data: &'a [u8],
        owner: &Pubkey,
    ) -> Result<&'a VaultInfo, ProgramError> {
        // Unpack and split data into discriminator & token_account
        let (disc, vault_info_bytes) = vault_info_data.split_at(8);

        // We only need to check the first byte
        if disc[0] != AccountDiscriminator::VaultInfo as u8 {
            log::sol_log("vault_info discriminator is incorrect");
            return Err(ProgramError::InvalidAccountData);
        }

        // Check account owner
        if solana_program::program_memory::sol_memcmp(
            owner.as_ref(),
            crate::ID.as_ref(),
            32,
        ) != 0
        {
            log::sol_log("vault_info has incorrect owner");
            return Err(ProgramError::IllegalOwner);
        }

        Ok(unsafe { &*(vault_info_bytes.as_ptr() as *const VaultInfo) })
    }
}
