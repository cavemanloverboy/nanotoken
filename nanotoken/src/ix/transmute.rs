use crate::solana_nostd_entrypoint::{InstructionC, NoStdAccountInfo};

use bytemuck::{Pod, Zeroable};
use solana_program::{
    log::{self},
    program_error::ProgramError,
    pubkey::Pubkey,
};

use crate::{
    error::NanoTokenError,
    utils::{
        spl_token_utils::{token::TokenAccountInfo, SPL_TOKEN_PROGRAM},
        split_at_unchecked,
    },
    Mint, TokenAccount, VaultInfo,
};

#[derive(PartialEq, Debug, Clone, Copy, Pod, Zeroable)]
#[repr(C)]
pub struct TransmuteArgs {
    pub amount: u64,
}

impl TransmuteArgs {
    pub fn from_data<'a>(
        data: &mut &'a [u8],
    ) -> Result<&'a TransmuteArgs, ProgramError> {
        const IX_LEN: usize = core::mem::size_of::<TransmuteArgs>();
        if data.len() >= IX_LEN {
            // SAFETY:
            // We do the length check ourselves instead of via
            // core::slice::split_at so we can return an error
            // instead of panicking.
            let (ix_data, rem) = unsafe { split_at_unchecked(data, IX_LEN) };
            *data = rem;

            // This is always aligned and all bit patterns are valid
            Ok(unsafe { &*(ix_data.as_ptr() as *const TransmuteArgs) })
        } else {
            Err(ProgramError::InvalidInstructionData)
        }
    }

    pub fn size() -> usize {
        core::mem::size_of::<Self>()
    }
}

pub fn transmute(
    accounts: &[NoStdAccountInfo],
    args: &TransmuteArgs,
) -> Result<usize, ProgramError> {
    // TODO docs
    let [from, to, owner, tokenkeg_mint, nanotoken_mint, vault_info, tokenkeg_vault, tokenkeg_program, _rem @ .., config, system_program, payer] =
        accounts
    else {
        log::sol_log("transmute expecting [from, to, owner, tokenkeg_mint, nanotoken_mint, .. ]");
        return Err(ProgramError::NotEnoughAccountKeys);
    };

    // Return early if Transmuting zero
    //
    // This is necessary!
    // It is extremely cheap implicit owner check for nanotoken from/to
    if args.amount == 0 {
        return Ok(8);
    }

    // Get vault info
    let vault_info_data = vault_info
        .try_borrow_data()
        .expect("first borrow will never fail");
    let vault_info_account =
        VaultInfo::checked_load(&vault_info_data, vault_info.owner())?;

    // Check nanotoken mint
    if solana_program::program_memory::sol_memcmp(
        nanotoken_mint.key().as_ref(),
        vault_info_account
            .nanotoken_mint
            .as_ref(),
        32,
    ) != 0
    {
        log::sol_log("nanotoken mint mismatch");
        return Err(ProgramError::InvalidArgument);
    }

    // Check tokenkeg mint
    if solana_program::program_memory::sol_memcmp(
        tokenkeg_mint.key().as_ref(),
        vault_info_account
            .tokenkeg_mint
            .as_ref(),
        32,
    ) != 0
    {
        log::sol_log("tokenkeg mint mismatch");
        return Err(ProgramError::InvalidArgument);
    }

    // Check tokenkeg vault
    if solana_program::program_memory::sol_memcmp(
        tokenkeg_vault.key().as_ref(),
        vault_info_account
            .tokenkeg_vault
            .as_ref(),
        32,
    ) != 0
    {
        log::sol_log("tokenkeg vault mismatch");
        return Err(ProgramError::InvalidArgument);
    }

    // Check tokenkeg program
    if solana_program::program_memory::sol_memcmp(
        tokenkeg_program.key().as_ref(),
        SPL_TOKEN_PROGRAM.as_ref(),
        32,
    ) != 0
    {
        log::sol_log("tokenkeg program mismatch");
        return Err(ProgramError::InvalidArgument);
    }

    // We will need nanotoken mint to increase or decrease supply
    let mut nanotoken_mint_data = nanotoken_mint
        .try_borrow_mut_data()
        .ok_or(NanoTokenError::DuplicateAccount)?;
    let nanotoken_mint_account =
        Mint::checked_load_mut(&mut nanotoken_mint_data)?;

    // Try to go tokenkeg -> nanotoken.
    // Tokenkeg will do authority check and balance check
    if let Ok(tokenkeg_from) = TokenAccountInfo::new_with_authority(
        from,
        tokenkeg_mint.key(),
        owner.key(),
        false,
    ) {
        {
            // Account owner check will be done implicitly by runtime
            let mut nanotoken_to_data = to
                .try_borrow_mut_data()
                .ok_or(NanoTokenError::DuplicateAccount)?;
            if let Ok(nanotoken_account) =
                TokenAccount::checked_load_mut(&mut nanotoken_to_data)
            {
                // Account is already initialized. Check nanotoken mint
                // 0) Check nanotoken account mint
                // 1) Increment nanotoken balance
                // 2) Increment nanotoken mint supply
                // 3) Transfer from tokenkeg to vault (later)

                // 0) Check nanotoken account mint
                assert!(
                    nanotoken_account.mint == nanotoken_mint_account.mint_index
                );

                // 1) Increment nanotoken balance
                nanotoken_account.balance += args.amount;

                // 2) Increment nanotoken mint supply
                nanotoken_mint_account.supply += args.amount;
            } else {
                // Account is not initialized
                // 1) initialize nanotoken account
                // 2) update nanotoken balance from 0 to amount
                // 3) Increment nanotoken mint supply
                // 4) Transfer from tokenkeg to vault (later)

                // 1) initialize nanotoken account
                // need to drop RefMut
                drop(nanotoken_to_data);

                // TODO: I am sad that we are calculating this bump on-chain
                //
                // The target_os = "solana" impl is alloc-free at least
                let account_bump = Pubkey::find_program_address(
                    &[
                        owner.key().as_ref(),
                        nanotoken_mint_account
                            .mint_index
                            .to_le_bytes()
                            .as_ref(),
                    ],
                    &crate::ID,
                )
                .1;

                log::sol_log("transmute: initializing nanotoken account");
                super::initialize_account::checked_initialize_account(
                    payer,
                    config,
                    to,
                    system_program,
                    owner.key(),
                    nanotoken_mint_account.mint_index,
                    account_bump,
                )?;

                // 2) update nanotoken balance from 0 to amount
                unsafe {
                    core::ptr::copy_nonoverlapping(
                        &args.amount as *const u64 as *const u8,
                        to.unchecked_borrow_mut_data()
                            .as_mut_ptr()
                            .add(48),
                        8,
                    );
                }

                // 3) Increment nanotoken mint supply
                nanotoken_mint_account.supply += args.amount;
            }

            // 4) Transfer from tokenkeg to vault
            // transfer has tag = 3, args = amount
            let mut tokenkeg_transfer_data = [3, 0, 0, 0, 0, 0, 0, 0, 0];
            unsafe {
                core::ptr::copy_nonoverlapping(
                    &args.amount as *const u64 as *const u8,
                    tokenkeg_transfer_data
                        .as_mut_ptr()
                        .add(1),
                    8,
                );
            }

            let infos = [
                tokenkeg_from.info.to_info_c(),
                tokenkeg_vault.to_info_c(),
                owner.to_info_c(),
            ];

            let tokenkeg_transfer_metas = [
                infos[0].to_meta_c(),
                tokenkeg_vault.to_meta_c(),
                owner.to_meta_c(),
            ];

            let transfer_ix = InstructionC {
                program_id: &SPL_TOKEN_PROGRAM,
                accounts: tokenkeg_transfer_metas.as_ptr(),
                accounts_len: 3,
                data: tokenkeg_transfer_data.as_ptr(),
                data_len: 9,
            };

            let cpi_seeds: &[&[&[u8]]] = &[];
            #[cfg(target_os = "solana")]
            unsafe {
                solana_program::syscalls::sol_invoke_signed_c(
                    &transfer_ix as *const InstructionC as *const u8,
                    infos.as_ptr() as *const u8,
                    3,
                    cpi_seeds.as_ptr() as *const u8,
                    0,
                );
            }
            #[cfg(not(target_os = "solana"))]
            core::hint::black_box((&transfer_ix, &infos, cpi_seeds));
        }
    } else {
        // Check to see if we can do nanotoken -> tokenkeg
        let mut from_data = from
            .try_borrow_mut_data()
            .expect("no other borrows exist"); // TODO: standardize comment
        let nanotoken_from = TokenAccount::checked_load_mut(&mut from_data)?;

        // Check for authority as signer
        if !owner.is_signer() {
            log::sol_log("from account owner must sign to transfer");
            return Err(ProgramError::MissingRequiredSignature);
        }
        if solana_program::program_memory::sol_memcmp(
            nanotoken_from.owner.as_ref(),
            owner.key().as_ref(),
            32,
        ) != 0
        {
            log::sol_log("incorrect from account owner");
            return Err(ProgramError::InvalidArgument);
        }

        // Check nanotoken balance
        if nanotoken_from.balance < args.amount {
            log::sol_log("insufficient balance");
            return Err(NanoTokenError::InsufficientTokenBalance.into());
        }

        // Reduce nanotoken balance, supply
        nanotoken_from.balance -= args.amount;
        nanotoken_mint_account.supply -= args.amount;

        // Transfer from vault to tokenkeg account
        let ta_exists =
            TokenAccountInfo::new(to, tokenkeg_mint.key(), false).is_ok();
        if !ta_exists {
            log::sol_log(
                "invalid tokenkeg/nanotoken account configuration for transmute",
            );
            return Err(ProgramError::InvalidArgument);
        }
        // Init if needed TODO
        //     // To initialize token account, we need to create account and then initialize it

        //     // CreateAccount {
        //     //   /// Number of lamports to transfer to the new account
        //     //   lamports: u64,

        //     //   /// Number of bytes of memory to allocate
        //     //   space: u64,

        //     //   /// Address of program that will own the new account
        //     //   owner: Pubkey,
        //     // },

        //     // discriminant is zero so we just need to write lamports, space, address
        //     let mut create_account_data = [0; 4 + 8 + 8 + 32];
        //     unsafe {
        //         // lamports
        //         let rent = Rent::get()?;
        //         core::ptr::copy_nonoverlapping(
        //             rent.minimum_balance(165)
        //                 .to_le_bytes()
        //                 .as_ptr(),
        //             create_account_data.as_mut_ptr().add(4),
        //             8,
        //         );

        //         // space
        //         core::ptr::copy_nonoverlapping(
        //             165_u64.to_le_bytes().as_ptr(),
        //             create_account_data.as_mut_ptr().add(12),
        //             8,
        //         );

        //         // owner
        //         core::ptr::copy_nonoverlapping(
        //             owner.key().as_ref().as_ptr(),
        //             create_account_data.as_mut_ptr().add(20),
        //             32,
        //         );
        //     }

        //     let create_account_accounts =
        // }

        // 4) Transfer from tokenkeg to vault
        // transfer has tag = 3, args = amount
        let mut tokenkeg_transfer_data = [3, 0, 0, 0, 0, 0, 0, 0, 0];
        unsafe {
            core::ptr::copy_nonoverlapping(
                &args.amount as *const u64 as *const u8,
                tokenkeg_transfer_data
                    .as_mut_ptr()
                    .add(1),
                8,
            );
        }

        let infos = [
            tokenkeg_vault.to_info_c(),
            to.to_info_c(),
            vault_info.to_info_c(),
        ];

        let tokenkeg_transfer_metas = [
            infos[0].to_meta_c(),
            to.to_meta_c(),
            vault_info.to_meta_c_signer(),
        ];

        let transfer_ix = InstructionC {
            program_id: &SPL_TOKEN_PROGRAM,
            accounts: tokenkeg_transfer_metas.as_ptr(),
            accounts_len: 3,
            data: tokenkeg_transfer_data.as_ptr(),
            data_len: 9,
        };

        let cpi_seeds: &[&[&[u8]]] = &[&[
            b"info",
            tokenkeg_mint.key().as_ref(),
            &[vault_info_account.info_bump],
        ]];
        #[cfg(target_os = "solana")]
        unsafe {
            solana_program::syscalls::sol_invoke_signed_c(
                &transfer_ix as *const InstructionC as *const u8,
                infos.as_ptr() as *const u8,
                3,
                cpi_seeds.as_ptr() as *const u8,
                1,
            );
        }
        #[cfg(not(target_os = "solana"))]
        core::hint::black_box((&transfer_ix, &infos, cpi_seeds));
    }

    Ok(8)
}
