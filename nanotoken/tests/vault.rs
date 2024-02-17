//! An end-to-end integration test

use std::{env, error::Error, path::Path};

use nanotoken::{
    ix::{
        InitializeAccountArgs, InitializeVaultArgs, Tag, TransferArgs,
        TransmuteArgs,
    },
    Mint, ProgramConfig, TokenAccount, VaultInfo,
};
use solana_program::{
    instruction::{AccountMeta, Instruction},
    native_token::LAMPORTS_PER_SOL,
    program_pack::Pack,
    rent::Rent,
    system_instruction, system_program,
};
use solana_program_test::ProgramTest;
use solana_sdk::{
    signature::{read_keypair_file, Keypair},
    signer::Signer,
    system_transaction,
    transaction::Transaction,
};

/// 1. Set up program environment and nanotoken program (initialize config)
/// 2. Initialize Tokenkeg token mint, token account, and mint Tokenkeg token to token account
/// 3. Create nanotoken vault, nanotoken accounts, and port over
/// 4. nanotoken transfer back and forth
/// 5. port back over to Tokenkeg
#[tokio::test(flavor = "current_thread")]
async fn round_trip() -> Result<(), Box<dyn Error>> {
    // 1. Set up program environment and nanotoken program (initialize config)
    let mut program_test = ProgramTest::new("nanotoken", nanotoken::ID, None);
    program_test.prefer_bpf(true);
    let mut ctx = program_test.start_with_context().await;
    let rent = Rent::default();

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

    // 2. Initialize Tokenkeg token mint, token account, and mint Tokenkeg token to token account
    let tokenkeg_mint = Keypair::new();
    let tokenkeg_account = Keypair::new();
    let create_tokenkeg_mint_account_ix = system_instruction::create_account(
        &ctx.payer.pubkey(),
        &tokenkeg_mint.pubkey(),
        rent.minimum_balance(82),
        82,
        &spl_token::ID,
    );
    let init_mint_ix = spl_token::instruction::initialize_mint2(
        &spl_token::ID,
        &tokenkeg_mint.pubkey(),
        &ctx.payer.pubkey(),
        None,
        6,
    )?;
    let create_tokenkeg_account_ix = system_instruction::create_account(
        &ctx.payer.pubkey(),
        &tokenkeg_account.pubkey(),
        rent.minimum_balance(165),
        165,
        &spl_token::ID,
    );
    let init_tokenkeg_account_ix = spl_token::instruction::initialize_account3(
        &spl_token::ID,
        &tokenkeg_account.pubkey(),
        &tokenkeg_mint.pubkey(),
        &ctx.payer.pubkey(),
    )?;
    let mint_to_tokenkeg_account_ix = spl_token::instruction::mint_to(
        &spl_token::ID,
        &tokenkeg_mint.pubkey(),
        &tokenkeg_account.pubkey(),
        &ctx.payer.pubkey(),
        &[&ctx.payer.pubkey()],
        1_000_000,
    )?;
    let transaction = Transaction::new_signed_with_payer(
        &[
            create_tokenkeg_mint_account_ix,
            init_mint_ix,
            create_tokenkeg_account_ix,
            init_tokenkeg_account_ix,
            mint_to_tokenkeg_account_ix,
        ],
        Some(&ctx.payer.pubkey()),
        &[&ctx.payer, &tokenkeg_mint, &tokenkeg_account],
        ctx.last_blockhash,
    );
    ctx.banks_client
        .process_transaction(transaction)
        .await?;

    // pre-3: fund a second user
    let second_user = Keypair::new();
    let fund_user_tx = system_transaction::transfer(
        &ctx.payer,
        &second_user.pubkey(),
        100 * LAMPORTS_PER_SOL,
        ctx.last_blockhash,
    );
    ctx.banks_client
        .process_transaction(fund_user_tx)
        .await?;

    // 3. Create nanotoken vault, nanotoken accounts, and port over
    let (vault, vault_bump) = VaultInfo::vault(&tokenkeg_mint.pubkey());
    let (info, info_bump) = VaultInfo::info(&tokenkeg_mint.pubkey());
    let mut step_4_data = vec![
        0;
        (8 + InitializeVaultArgs::size())
            + 2 * (8 + InitializeAccountArgs::size())
            + (8 + TransmuteArgs::size())
    ];
    let nanotoken_mint = Keypair::new();
    let (nanotoken_account_1, nanotoken_bump_1) =
        TokenAccount::address(0, &ctx.payer.pubkey());
    let (nanotoken_account_2, nanotoken_bump_2) =
        TokenAccount::address(0, &second_user.pubkey());
    // ix 1: init vault
    step_4_data[0] = Tag::InitializeVault as u8;
    step_4_data[8..12].copy_from_slice(&(info_bump as u32).to_le_bytes());
    step_4_data[12..16].copy_from_slice(&(vault_bump as u32).to_le_bytes());
    // ix 2: create account
    step_4_data[16] = Tag::InitializeAccount as u8;
    step_4_data[24..56].copy_from_slice(ctx.payer.pubkey().as_ref());
    // TODO switch to key
    step_4_data[56..64].copy_from_slice(0_u64.to_le_bytes().as_ref());
    step_4_data[64..72].copy_from_slice(
        (nanotoken_bump_1 as u64)
            .to_le_bytes()
            .as_ref(),
    );
    // ix 3: create account
    step_4_data[72] = Tag::InitializeAccount as u8;
    step_4_data[80..112].copy_from_slice(second_user.pubkey().as_ref());
    // TODO switch to key
    step_4_data[112..120].copy_from_slice(0_u64.to_le_bytes().as_ref());
    step_4_data[120..128].copy_from_slice(
        (nanotoken_bump_2 as u64)
            .to_le_bytes()
            .as_ref(),
    );
    // ix 4: transmute
    step_4_data[128] = Tag::Transmute as u8;
    step_4_data[136..144].copy_from_slice(10_u64.to_le_bytes().as_ref());

    let pre_token_balance = spl_token::state::Account::unpack(
        &ctx.banks_client
            .get_account(tokenkeg_account.pubkey())
            .await?
            .unwrap()
            .data,
    )
    .unwrap()
    .amount;
    let step_4_accounts = vec![
        // create vault
        AccountMeta::new_readonly(tokenkeg_mint.pubkey(), false),
        AccountMeta::new(vault, false),
        AccountMeta::new_readonly(spl_token::ID, false),
        AccountMeta::new(info, false),
        AccountMeta::new(nanotoken_mint.pubkey(), false),
        // create account
        AccountMeta::new(nanotoken_account_1, false),
        // create account
        AccountMeta::new(nanotoken_account_2, false),
        // transmute
        // from, to, owner, tokenkeg_mint, nanotoken_mint, vault_info, tokenkeg_vault, tokenkeg_program
        AccountMeta::new(tokenkeg_account.pubkey(), false),
        AccountMeta::new(nanotoken_account_1, false),
        AccountMeta::new(ctx.payer.pubkey(), true),
        AccountMeta::new(tokenkeg_mint.pubkey(), false),
        AccountMeta::new(nanotoken_mint.pubkey(), false),
        AccountMeta::new_readonly(info, false),
        AccountMeta::new(vault, false),
        AccountMeta::new_readonly(spl_token::ID, false),
        // config
        AccountMeta::new(config, false),
        AccountMeta::new_readonly(system_program::ID, false),
        AccountMeta::new(ctx.payer.pubkey(), true),
    ];
    let create_nanotoken_mint = system_instruction::create_account(
        &ctx.payer.pubkey(),
        &nanotoken_mint.pubkey(),
        Rent::default().minimum_balance(Mint::space()),
        Mint::space() as u64,
        &nanotoken::ID,
    );
    let create_nanotoken_vault_nano_token_accounts_and_port_over =
        Instruction {
            program_id: nanotoken::ID,
            accounts: step_4_accounts,
            data: step_4_data,
        };
    let transaction = Transaction::new_signed_with_payer(
        &[
            create_nanotoken_mint,
            create_nanotoken_vault_nano_token_accounts_and_port_over,
        ],
        Some(&ctx.payer.pubkey()),
        &[&ctx.payer, &nanotoken_mint],
        ctx.last_blockhash,
    );
    ctx.banks_client
        .process_transaction(transaction)
        .await
        .unwrap();
    let onchain_tokenkeg_account = ctx
        .banks_client
        .get_account(tokenkeg_account.pubkey())
        .await?
        .unwrap();
    let post_token_balance =
        spl_token::state::Account::unpack(&onchain_tokenkeg_account.data)
            .unwrap()
            .amount;
    println!(
        "tokenkeg account owner = {}",
        onchain_tokenkeg_account.owner
    );

    let post_nanotoken_balance = u64::from_le_bytes(
        ctx.banks_client
            .get_account(nanotoken_account_1)
            .await?
            .unwrap()
            .data
            .get(48..56)
            .unwrap()
            .try_into()
            .unwrap(),
    );
    println!("tokenkeg account before/after transmute = {pre_token_balance}/{post_token_balance}");
    println!(
        "nanotoken account before/after transmute = 0/{post_nanotoken_balance}"
    );

    // 5. nanotoken transfer back and forth
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
                AccountMeta::new(nanotoken_account_1, false),
                AccountMeta::new(nanotoken_account_2, false),
                AccountMeta::new_readonly(ctx.payer.pubkey(), true),
            ])
        } else {
            accounts.extend([
                AccountMeta::new(nanotoken_account_2, false),
                AccountMeta::new(nanotoken_account_1, false),
                AccountMeta::new_readonly(second_user.pubkey(), true),
            ])
        }
    }
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
    ctx.banks_client
        .process_transaction(transaction)
        .await
        .unwrap();

    // 6. port back over to Tokenkeg
    let step_6_accounts = vec![
        // transmute
        // from, to, owner, tokenkeg_mint, nanotoken_mint, vault_info, tokenkeg_vault, tokenkeg_program, _rem @ .., config, system_program, payer
        dbg!(AccountMeta::new(nanotoken_account_1, false)),
        dbg!(AccountMeta::new(tokenkeg_account.pubkey(), false)),
        dbg!(AccountMeta::new(ctx.payer.pubkey(), true)),
        dbg!(AccountMeta::new(tokenkeg_mint.pubkey(), false)),
        dbg!(AccountMeta::new(nanotoken_mint.pubkey(), false)),
        dbg!(AccountMeta::new_readonly(info, false)),
        dbg!(AccountMeta::new(vault, false)),
        dbg!(AccountMeta::new_readonly(spl_token::ID, false)),
        // config
        AccountMeta::new(config, false),
        AccountMeta::new_readonly(system_program::ID, false),
        AccountMeta::new(ctx.payer.pubkey(), true),
    ];
    let mut step_6_data = vec![];
    step_6_data.extend((Tag::Transmute as u64).to_le_bytes());
    step_6_data.extend((10_u64).to_le_bytes());
    let port_back = Instruction {
        program_id: nanotoken::ID,
        accounts: step_6_accounts,
        data: step_6_data,
    };
    let transaction = Transaction::new_signed_with_payer(
        &[port_back],
        Some(&ctx.payer.pubkey()),
        &[&ctx.payer],
        ctx.last_blockhash,
    );
    ctx.banks_client
        .process_transaction(transaction)
        .await
        .unwrap();

    Ok(())
}
