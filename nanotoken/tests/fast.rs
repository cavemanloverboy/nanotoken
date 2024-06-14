//! An end-to-end integration test

use std::{env, error::Error, path::Path};

use nanotoken::{
    ix::{InitializeAccountArgs, InitializeMintArgs, MintArgs, Tag},
    Mint, ProgramConfig, TokenAccount,
};
use solana_program::{
    instruction::{AccountMeta, Instruction},
    native_token::LAMPORTS_PER_SOL,
    pubkey::Pubkey,
    rent::Rent,
    system_program,
};
use solana_program_test::{BanksClient, ProgramTest};
use solana_sdk::{
    signature::{read_keypair_file, Keypair},
    signer::Signer,
    system_transaction,
    transaction::Transaction,
};

#[tokio::test(flavor = "current_thread")]
async fn fast_xfer() -> Result<(), Box<dyn Error>> {
    let mut program_test = ProgramTest::default();
    program_test.prefer_bpf(true);
    program_test.add_program("nanotoken", nanotoken::ID, None);
    let mut ctx = program_test.start_with_context().await;

    // Initialize config
    let config_keypair = read_keypair_file(
        Path::new(&env::var("CARGO_MANIFEST_DIR").unwrap())
            .parent()
            .unwrap()
            .join("config.json"),
    )
    .unwrap();
    let config = config_keypair.pubkey();
    let create_config = system_transaction::create_account(
        &ctx.payer,
        &config_keypair,
        ctx.last_blockhash,
        Rent::default().minimum_balance(ProgramConfig::space()),
        ProgramConfig::space() as u64,
        &nanotoken::ID,
    );
    ctx.banks_client
        .process_transaction(create_config)
        .await
        .unwrap();

    // Initialize mint
    let mint_keypair = Keypair::new();
    let mint = mint_keypair.pubkey();
    let create_mint = system_transaction::create_account(
        &ctx.payer,
        &mint_keypair,
        ctx.last_blockhash,
        Rent::default().minimum_balance(Mint::space()),
        Mint::space() as u64,
        &nanotoken::ID,
    );
    ctx.banks_client
        .process_transaction(create_mint)
        .await
        .unwrap();

    // Initialize config
    let ix_data = (Tag::InitializeConfig as u64)
        .to_le_bytes()
        .to_vec();

    let accounts = vec![
        AccountMeta::new(config, false),
        AccountMeta::new_readonly(system_program::ID, false),
        AccountMeta::new_readonly(ctx.payer.pubkey(), false),
    ];
    let instruction = Instruction {
        program_id: nanotoken::ID,
        accounts,
        data: ix_data,
    };
    let transaction = Transaction::new_signed_with_payer(
        &[instruction],
        Some(&ctx.payer.pubkey()),
        &[&ctx.payer],
        ctx.last_blockhash,
    );
    ctx.banks_client
        .process_transaction(transaction)
        .await
        .unwrap();

    // Initialize mint
    let mut ix_data = vec![0; 8 + InitializeMintArgs::size()];
    ix_data[0..8].copy_from_slice(&(Tag::InitializeMint as u64).to_le_bytes());
    let InitializeMintArgs {
        authority,
        decimals,
    } = bytemuck::try_from_bytes_mut(&mut ix_data[8..]).unwrap();
    *authority = ctx.payer.pubkey();
    *decimals = 6;

    let accounts = vec![
        AccountMeta::new(mint, false),
        AccountMeta::new(config, false),
        AccountMeta::new_readonly(system_program::ID, false),
        AccountMeta::new(ctx.payer.pubkey(), false),
    ];
    let instruction = Instruction {
        program_id: nanotoken::ID,
        accounts,
        data: ix_data,
    };
    let transaction = Transaction::new_signed_with_payer(
        &[instruction],
        Some(&ctx.payer.pubkey()),
        &[&ctx.payer],
        ctx.last_blockhash,
    );

    ctx.banks_client
        .process_transaction(transaction)
        .await
        .unwrap();

    // Initialize token account AND mint
    let mut ix_data =
        vec![0; 8 + InitializeAccountArgs::size() + 8 + MintArgs::size()];
    let (token_account, token_account_bump) =
        TokenAccount::address(0, &ctx.payer.pubkey());
    {
        ix_data[0..8]
            .copy_from_slice(&(Tag::InitializeAccount as u64).to_le_bytes());
        let InitializeAccountArgs { owner, mint, bump } =
            bytemuck::try_from_bytes_mut(
                &mut ix_data[8..8 + InitializeAccountArgs::size()],
            )
            .unwrap();
        *owner = ctx.payer.pubkey();
        *mint = 0;
        *bump = token_account_bump as u64;
        ix_data[8 + InitializeAccountArgs::size()
            ..8 + InitializeAccountArgs::size() + 8]
            .copy_from_slice(&(Tag::Mint as u64).to_le_bytes());
        let MintArgs { amount } = bytemuck::try_from_bytes_mut(
            &mut ix_data[8 + InitializeAccountArgs::size() + 8..],
        )
        .unwrap();
        *amount = 1000;
    }
    let accounts = vec![
        // create
        AccountMeta::new(token_account, false),
        // mint
        AccountMeta::new(token_account, false),
        AccountMeta::new(mint, false),
        AccountMeta::new(ctx.payer.pubkey(), true),
        // remainder
        AccountMeta::new(config, false),
        AccountMeta::new_readonly(system_program::ID, false),
        AccountMeta::new(ctx.payer.pubkey(), true),
    ];
    let instruction = Instruction {
        program_id: nanotoken::ID,
        accounts,
        data: ix_data,
    };
    let transaction = Transaction::new_signed_with_payer(
        &[instruction],
        Some(&ctx.payer.pubkey()),
        &[&ctx.payer],
        ctx.last_blockhash,
    );
    println!("payer = {}", ctx.payer.pubkey());
    println!("token account = {token_account}");

    ctx.banks_client
        .process_transaction(transaction)
        .await
        .unwrap();

    // check state
    let user_token_account =
        get_nanotoken_account(&mut ctx.banks_client, token_account).await?;
    assert_eq!(user_token_account.mint, 0);
    assert_eq!(user_token_account.owner, ctx.payer.pubkey());
    assert_eq!(user_token_account.balance, 1000);

    // Initialize a second token account
    // First fund a second user
    let second_user = Keypair::new();
    ctx.banks_client
        .process_transaction(system_transaction::transfer(
            &ctx.payer,
            &second_user.pubkey(),
            5 * LAMPORTS_PER_SOL,
            ctx.last_blockhash,
        ))
        .await
        .unwrap();
    // Now create token account (test semi-funded case with transfer prior to invocation)
    let mut ix_data = vec![0; 8 + InitializeAccountArgs::size()];
    let (second_token_account, token_account_bump) =
        TokenAccount::address(0, &second_user.pubkey());
    {
        ix_data[0..8]
            .copy_from_slice(&(Tag::InitializeAccount as u64).to_le_bytes());
        let InitializeAccountArgs { owner, mint, bump } =
            bytemuck::try_from_bytes_mut(
                &mut ix_data[8..8 + InitializeAccountArgs::size()],
            )
            .unwrap();
        *owner = second_user.pubkey();
        *mint = 0;
        *bump = token_account_bump as u64;
    }
    // This is the pre-transfer ix
    let pre_transfer_ix = solana_program::system_instruction::transfer(
        &ctx.payer.pubkey(),
        &second_token_account,
        Rent::default().minimum_balance(0),
    );
    let accounts = vec![
        // create
        AccountMeta::new(second_token_account, false),
        // remainder
        AccountMeta::new(config, false),
        AccountMeta::new_readonly(system_program::ID, false),
        AccountMeta::new(ctx.payer.pubkey(), true),
    ];
    let instruction = Instruction {
        program_id: nanotoken::ID,
        accounts,
        data: ix_data,
    };
    let transaction = Transaction::new_signed_with_payer(
        &[pre_transfer_ix, instruction],
        Some(&ctx.payer.pubkey()),
        &[&ctx.payer],
        ctx.last_blockhash,
    );
    println!("payer = {}", ctx.payer.pubkey());
    println!("token account = {second_token_account}");

    ctx.banks_client
        .process_transaction(transaction)
        .await
        .unwrap();

    // fast transfer
    let mut ix_data = vec![0; 8];
    {
        ix_data[0..8].copy_from_slice(&5_u64.to_le_bytes());
    }
    let accounts = vec![
        // transfer
        AccountMeta::new(token_account, false),
        AccountMeta::new(second_token_account, false),
        AccountMeta::new_readonly(ctx.payer.pubkey(), true),
    ];
    let instruction = Instruction {
        program_id: nanotoken::ID,
        accounts,
        data: ix_data,
    };
    let transaction = Transaction::new_signed_with_payer(
        &[instruction],
        Some(&ctx.payer.pubkey()),
        &[&ctx.payer],
        ctx.last_blockhash,
    );
    println!("payer = {}", ctx.payer.pubkey());
    println!("token account = {second_token_account}");

    ctx.banks_client
        .process_transaction(transaction)
        .await
        .unwrap();

    // check state (transfered 5 atoms)
    let user_token_account =
        get_nanotoken_account(&mut ctx.banks_client, token_account).await?;
    assert_eq!(user_token_account.balance, 995);
    let second_user_token_account =
        get_nanotoken_account(&mut ctx.banks_client, second_token_account)
            .await?;
    assert_eq!(second_user_token_account.mint, 0);
    assert_eq!(second_user_token_account.owner, second_user.pubkey());
    assert_eq!(second_user_token_account.balance, 5);

    Ok(())
}

pub async fn get_nanotoken_account(
    client: &mut BanksClient,
    key: Pubkey,
) -> Result<TokenAccount, Box<dyn Error>> {
    use core::mem::MaybeUninit;
    let mut ta = MaybeUninit::uninit();
    let account = client
        .get_account(key)
        .await?
        .ok_or("could not find account")?;

    unsafe {
        core::ptr::copy_nonoverlapping(
            account.data.as_ptr().add(8),
            ta.as_mut_ptr() as *mut u8,
            core::mem::size_of::<TokenAccount>(),
        );

        Ok(ta.assume_init())
    }
}
