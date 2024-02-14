//! An end-to-end integration test

use std::{env, error::Error, path::Path};

use nanotoken::{
    ix::{
        InitializeAccountArgs, InitializeMintArgs, MintArgs, Tag, TransferArgs,
    },
    Mint, ProgramConfig, TokenAccount,
};
use solana_program::{
    instruction::{AccountMeta, Instruction},
    native_token::LAMPORTS_PER_SOL,
    rent::Rent,
    system_program,
};
use solana_program_test::ProgramTest;
use solana_sdk::{
    signature::{read_keypair_file, Keypair},
    signer::Signer,
    system_transaction,
    transaction::Transaction,
};

#[tokio::test(flavor = "current_thread")]
async fn end_to_end() -> Result<(), Box<dyn Error>> {
    let mut program_test = ProgramTest::new("nanotoken", nanotoken::ID, None);
    program_test.prefer_bpf(true);
    let mut ctx = program_test.start_with_context().await;

    // Initialize config
    let config_keypair = read_keypair_file(
        Path::new(&env::var("CARGO_MANIFEST_DIR").unwrap()).join("config.json"),
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
    // Now create token account
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

    // transfer
    let mut ix_data = vec![0; 8 + TransferArgs::size()];
    {
        ix_data[0..8].copy_from_slice(&(Tag::Transfer as u64).to_le_bytes());
        let TransferArgs { amount } = bytemuck::try_from_bytes_mut(
            &mut ix_data[8..8 + TransferArgs::size()],
        )
        .unwrap();
        *amount = 5;
    }
    let accounts = vec![
        // transfer
        AccountMeta::new(token_account, false),
        AccountMeta::new(second_token_account, false),
        AccountMeta::new_readonly(ctx.payer.pubkey(), true),
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
    println!("token account = {second_token_account}");

    ctx.banks_client
        .process_transaction(transaction)
        .await
        .unwrap();

    // multi-transfer
    let num_transfers = 2;
    let mut ix_data = vec![0; num_transfers * (8 + TransferArgs::size())];
    let mut accounts = vec![];
    for n in 0..num_transfers {
        let disc_offset = 8 * n + n * TransferArgs::size();
        ix_data[disc_offset..8 + disc_offset]
            .copy_from_slice(&(Tag::Transfer as u64).to_le_bytes());
        let TransferArgs { amount } = bytemuck::try_from_bytes_mut(
            &mut ix_data
                [disc_offset + 8..disc_offset + 8 + TransferArgs::size()],
        )
        .unwrap();
        *amount = 1;

        if n % 2 == 0 {
            accounts.extend([
                AccountMeta::new(token_account, false),
                AccountMeta::new(second_token_account, false),
                AccountMeta::new_readonly(ctx.payer.pubkey(), true),
            ])
        } else {
            accounts.extend([
                AccountMeta::new(second_token_account, false),
                AccountMeta::new(token_account, false),
                AccountMeta::new_readonly(second_user.pubkey(), true),
            ])
        }
    }
    accounts.extend([
        // remainder
        AccountMeta::new(config, false),
        AccountMeta::new_readonly(system_program::ID, false),
        AccountMeta::new(ctx.payer.pubkey(), true),
    ]);
    let instruction = Instruction {
        program_id: nanotoken::ID,
        accounts,
        data: ix_data,
    };
    let transaction = Transaction::new_signed_with_payer(
        &[instruction],
        Some(&ctx.payer.pubkey()),
        &[&ctx.payer, &second_user],
        ctx.last_blockhash,
    );
    println!("payer = {}", ctx.payer.pubkey());
    println!("token account = {second_token_account}");

    ctx.banks_client
        .process_transaction(transaction)
        .await
        .unwrap();

    Ok(())
}
