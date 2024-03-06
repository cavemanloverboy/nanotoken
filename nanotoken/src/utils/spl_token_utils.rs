use crate::solana_nostd_entrypoint::{NoStdAccountInfo, Ref};
use solana_program::{
    log, program_error::ProgramError, program_option::COption, pubkey::Pubkey,
};

pub struct MintAccountInfo<'a> {
    pub info: &'a NoStdAccountInfo,
    pub data: Ref<'a, [u8]>,
    pub mint: &'a MintZC,
}

pub const SPL_TOKEN_PROGRAM: Pubkey =
    solana_program::pubkey!("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA");

impl<'a> MintAccountInfo<'a> {
    pub fn new(
        info: &'a NoStdAccountInfo,
    ) -> Result<MintAccountInfo<'a>, ProgramError> {
        // TODO cmp
        if *info.owner() != SPL_TOKEN_PROGRAM {
            log::sol_log("Mint account must be owned by the Token Program");
            return Err(ProgramError::IllegalOwner);
        }

        // Validate mint
        let data = info
            .try_borrow_data()
            .ok_or(ProgramError::AccountBorrowFailed)?;
        let _validated = MintZC::from_slice(&data).ok_or_else(|| {
            log::sol_log("invalid mint account");
            ProgramError::InvalidAccountData
        })?;

        Ok(Self {
            info,
            mint: unsafe { core::mem::transmute(&*data.as_ptr()) },
            data,
        })
    }
}

/// Mint data.
#[repr(C, packed)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MintZC {
    /// Optional authority used to mint new tokens. The mint authority may only be provided during
    /// mint creation. If no mint authority is present then the mint has a fixed supply and no
    /// further tokens may be minted.
    pub mint_authority: COption<Pubkey>,
    /// Total supply of tokens.
    pub supply: u64,
    /// Number of base 10 digits to the right of the decimal place.
    pub decimals: u8,
    /// Is `true` if this structure has been initialized
    pub is_initialized: bool,
    /// Optional authority to freeze token accounts.
    pub freeze_authority: COption<Pubkey>,
}

impl MintZC {
    pub fn from_slice<'d>(data: &'d [u8]) -> Option<&'d MintZC> {
        let mut ptr = data.as_ptr();

        unsafe {
            // Check mint authority discriminant
            ptr = ptr.add(check_copt_disc(ptr as *const u32)?);

            // Skip over supply, decimals
            ptr = ptr.add(9);

            // Check mint is initialized
            if *ptr != 1 {
                return None;
            }
            ptr = ptr.add(1);

            // Check freeze authority disriminant
            check_copt_disc(ptr as *const u32)?;

            Some(core::mem::transmute(&*data.as_ptr()))
        }
    }
}

// returns offset to next element
unsafe fn check_copt_disc(ptr: *const u32) -> Option<usize> {
    match *ptr {
        // None or Some
        0 | 1 => Some(36),

        _ => None,
    }
}

#[test]
fn mint_zc() {
    if cfg!(target_endian = "little") {
        #[rustfmt::skip]
        let mint_zc_data = [
            // Some(key)
            1, 0, 0, 0,
            1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16,
            1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16,

            // supply
            128, 0, 0, 0, 0, 0, 0, 0,

            // decimals
            6,

            // init
            1,

            // Some(key)
            1, 0, 0, 0,
            1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16,
            1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16,
        ];

        let mint_zc = MintZC::from_slice(&mint_zc_data).unwrap();
        #[rustfmt::skip]
        let expected_auth = COption::Some(Pubkey::new_from_array([
            1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16,
            1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16,
        ]));
        let ma = mint_zc.mint_authority;
        assert_eq!(ma, expected_auth);
        let supply = mint_zc.supply;
        assert_eq!(supply, 128);
        assert_eq!(mint_zc.decimals, 6);
        let fa = mint_zc.freeze_authority;
        assert_eq!(fa, expected_auth);

        #[rustfmt::skip]
        let mint_zc_data = [
            // None
            0, 0, 0, 0,
            1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16,
            1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16,

            // supply
            128, 0, 0, 0, 0, 0, 0, 0,

            // decimals
            6,

            // init
            1,

            // None
            0, 0, 0, 0,
            1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16,
            1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16,
        ];

        let mint_zc = MintZC::from_slice(&mint_zc_data).unwrap();
        let expected_auth = COption::None;
        let ma = mint_zc.mint_authority;
        assert_eq!(ma, expected_auth);
        let supply = mint_zc.supply;
        assert_eq!(supply, 128);
        assert_eq!(mint_zc.decimals, 6);
        let fa = mint_zc.freeze_authority;
        assert_eq!(fa, expected_auth);
    } else {
        // TODO
    }
}

pub mod token {
    use crate::solana_nostd_entrypoint::NoStdAccountInfo;
    use solana_program::{log, program_error::ProgramError, pubkey::Pubkey};

    use crate::error::NanoTokenError;

    use super::SPL_TOKEN_PROGRAM;

    #[derive(Clone)]
    pub struct TokenAccountInfo<'a> {
        pub info: &'a NoStdAccountInfo,
    }

    pub const TOKENKEG_ACCOUNT_LEN: usize = 165;

    impl<'a> TokenAccountInfo<'a> {
        pub fn new(
            info: &'a NoStdAccountInfo,
            mint: &Pubkey,
            print: bool,
        ) -> Result<TokenAccountInfo<'a>, ProgramError> {
            // Check account is owned by spl token program
            if solana_program::program_memory::sol_memcmp(
                info.owner().as_ref(),
                SPL_TOKEN_PROGRAM.as_ref(),
                32,
            ) != 0
            {
                if print {
                    log::sol_log(
                        "Token account must be owned by the Token Program",
                    );
                }
                return Err(ProgramError::IllegalOwner);
            }

            // Check account data is correct length
            if info.data_len() != TOKENKEG_ACCOUNT_LEN {
                if print {
                    log::sol_log("Token account data length must be 165 bytes");
                }
                return Err(ProgramError::InvalidAccountData);
            }

            // Check token mint is correct
            if solana_program::program_memory::sol_memcmp(
                info.try_borrow_data()
                    .ok_or(NanoTokenError::DuplicateAccount)?
                    .get(0..32)
                    .ok_or(ProgramError::AccountDataTooSmall)?,
                mint.as_ref(),
                32,
            ) != 0
            {
                if print {
                    log::sol_log("Token account mint mismatch");
                }
                return Err(ProgramError::InvalidAccountData);
            }

            Ok(Self { info })
        }

        pub fn new_with_authority(
            info: &'a NoStdAccountInfo,
            mint: &Pubkey,
            authority: &Pubkey,
            print: bool,
        ) -> Result<TokenAccountInfo<'a>, ProgramError> {
            let token_account_info = Self::new(info, mint, print)?;

            // Check with authority
            if solana_program::program_memory::sol_memcmp(
                info.try_borrow_data()
                    .ok_or(NanoTokenError::DuplicateAccount)?
                    .get(32..64)
                    .ok_or(ProgramError::InvalidAccountData)?,
                authority.as_ref(),
                32,
            ) != 0
            {
                if print {
                    log::sol_log("Token account owner mismatch");
                }
                return Err(ProgramError::IllegalOwner);
            }
            Ok(token_account_info)
        }
    }
}
