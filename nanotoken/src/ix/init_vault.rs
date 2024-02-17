use bytemuck::{Pod, Zeroable};
use solana_nostd_entrypoint::{InstructionC, NoStdAccountInfo4};
use solana_program::{
    entrypoint::ProgramResult, log, program_error::ProgramError, pubkey::Pubkey,
};

use crate::{
    utils::{
        create_pda_funded_by_payer,
        spl_token_utils::{MintAccountInfo, SPL_TOKEN_PROGRAM},
        split_at_unchecked,
    },
    AccountDiscriminator, VaultInfo,
};

#[derive(PartialEq, Debug, Clone, Copy, Pod, Zeroable)]
#[repr(C)]
pub struct InitializeVaultArgs {
    info_bump: u32,
    vault_bump: u32,
}

impl InitializeVaultArgs {
    pub fn from_data<'a>(
        data: &mut &'a [u8],
    ) -> Result<&'a InitializeVaultArgs, ProgramError> {
        const IX_LEN: usize = core::mem::size_of::<InitializeVaultArgs>();
        if data.len() >= IX_LEN {
            // SAFETY:
            // We do the length check ourselves instead of via
            // core::slice::split_at so we can return an error
            // instead of panicking.
            let (ix_data, rem) = unsafe { split_at_unchecked(data, IX_LEN) };
            *data = rem;

            // This is always aligned and all bit patterns are valid
            Ok(unsafe { &*(ix_data.as_ptr() as *const InitializeVaultArgs) })
        } else {
            Err(ProgramError::InvalidInstructionData)
        }
    }

    pub const fn size() -> usize {
        core::mem::size_of::<Self>()
    }
}

pub fn initialize_vault(
    accounts: &[NoStdAccountInfo4],
    args: &InitializeVaultArgs,
) -> Result<usize, ProgramError> {
    // TODO DOCS AND VALIDATION
    // Unpack accounts
    //
    // 1) tokenkeg_mint will be checked by create_token_account
    // 2) Config will be checked
    // 4) payer will be checked by the sol transfer if necessary
    let [tokenkeg_mint, tokenkeg_vault, tokenkeg_program, vault_info, nanotoken_mint, _rem @ .., config, system_program, payer] =
        accounts
    else {
        log::sol_log(
            "expecting token_account, ... config, system_program, payer",
        );
        return Err(ProgramError::NotEnoughAccountKeys);
    };

    // Validate token mint
    let tokenkeg_mint_info = MintAccountInfo::new(&tokenkeg_mint)?;

    // Create vault_info
    create_vault_info(
        tokenkeg_mint,
        vault_info,
        system_program,
        payer,
        tokenkeg_vault,
        nanotoken_mint.key(),
        args.info_bump as u8,
    )?;

    // Create spl token vault
    initialize_program_owned_spl_vault(
        tokenkeg_mint,
        tokenkeg_vault,
        tokenkeg_program,
        payer,
        vault_info,
        system_program,
        args.vault_bump as u8,
    )?;

    // Create nano token mint
    super::initialize_mint::checked_initialized_mint(
        config,
        nanotoken_mint,
        vault_info.key(),
        &(tokenkeg_mint_info.mint.decimals as u64),
    )?;

    Ok(5)
}

fn create_vault_info(
    tokenkeg_mint: &NoStdAccountInfo4,
    vault_info: &NoStdAccountInfo4,
    system_program: &NoStdAccountInfo4,
    payer: &NoStdAccountInfo4,
    tokenkeg_vault: &NoStdAccountInfo4,
    nanotoken_mint: &Pubkey,
    info_bump: u8,
) -> Result<(), ProgramError> {
    // Create vault info account
    let pda_seeds = &[b"info", tokenkeg_mint.key().as_ref(), &[info_bump]];
    create_pda_funded_by_payer(
        vault_info.to_info_c(),
        &crate::ID,
        VaultInfo::space() as u64,
        pda_seeds,
        system_program.to_info_c(),
        payer.to_info_c(),
    )?;

    // Initialize vault info
    unsafe {
        let vault_info_account_data = vault_info
            .unchecked_borrow_mut_data()
            .as_mut_ptr();

        // Write discriminator
        *vault_info_account_data = AccountDiscriminator::VaultInfo as u8;

        // Write spl mint
        core::ptr::copy_nonoverlapping(
            tokenkeg_mint.key().as_ref().as_ptr(),
            vault_info_account_data.add(8),
            32,
        );

        // Write spl vault
        core::ptr::copy_nonoverlapping(
            tokenkeg_vault.key().as_ref().as_ptr(),
            vault_info_account_data.add(40),
            32,
        );

        // Write nanotoken mint (will be initialized by end of ix)
        core::ptr::copy_nonoverlapping(
            nanotoken_mint.as_ref().as_ptr(),
            vault_info_account_data.add(72),
            32,
        );

        // Write info bump
        *vault_info_account_data.add(104) = info_bump;
    };
    Ok(())
}

fn initialize_program_owned_spl_vault(
    tokenkeg_mint: &NoStdAccountInfo4,
    tokenkeg_vault: &NoStdAccountInfo4,
    tokenkeg_program: &NoStdAccountInfo4,
    payer: &NoStdAccountInfo4,
    vault_info: &NoStdAccountInfo4,
    system_program: &NoStdAccountInfo4,
    vault_bump: u8,
) -> ProgramResult {
    // Create account, initialize account
    let vault_seeds = [b"vault", tokenkeg_mint.key().as_ref(), &[vault_bump]];
    create_pda_funded_by_payer(
        tokenkeg_vault.to_info_c(),
        tokenkeg_program.key(),
        165,
        &vault_seeds,
        system_program.to_info_c(),
        payer.to_info_c(),
    )?;

    // Initialize account
    let mut data = [0; 33];
    unsafe {
        *data.as_mut_ptr() = 18;
        core::ptr::copy_nonoverlapping(
            vault_info.key().as_ref().as_ptr(),
            data.as_mut_ptr().add(1),
            32,
        );
    }
    let accounts = [tokenkeg_vault.to_meta_c(), tokenkeg_mint.to_meta_c()];
    let init_account_ix = InstructionC {
        program_id: &SPL_TOKEN_PROGRAM,
        accounts: accounts.as_ptr(),
        accounts_len: 2,
        data: data.as_mut_ptr(),
        data_len: 33,
    };
    let infos = [
        tokenkeg_vault.to_info_c(),
        tokenkeg_mint.to_info_c(),
        tokenkeg_program.to_info_c(),
    ];
    let cpi_seeds: &[&[&[u8]]] = &[&vault_seeds];
    #[cfg(target_os = "solana")]
    unsafe {
        solana_program::syscalls::sol_invoke_signed_c(
            &init_account_ix as *const InstructionC as *const u8,
            infos.as_ptr() as *const u8,
            3,
            cpi_seeds.as_ptr() as *const u8,
            0,
        );
    }
    #[cfg(not(target_os = "solana"))]
    core::hint::black_box((&init_account_ix, &infos, cpi_seeds));

    Ok(())
}
