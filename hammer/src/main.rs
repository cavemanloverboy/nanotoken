use std::{
    env,
    error::Error,
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, RwLock,
    },
    time::{Duration, Instant},
};

use clap::{Parser, Subcommand};
use indicatif::{ProgressBar, ProgressStyle};
use nanotoken::{
    ix::{
        InitializeAccountArgs, InitializeMintArgs, MintArgs, Tag, TransferArgs,
    },
    Mint, ProgramConfig, TokenAccount,
};
use solana_client::{
    nonblocking::{rpc_client::RpcClient, tpu_client::TpuClient}, tpu_client::TpuClientConfig
};
use solana_cost_model::cost_tracker::CostTracker;
use solana_sdk::{
    commitment_config::CommitmentConfig,
    compute_budget::ComputeBudgetInstruction,
    feature_set::FeatureSet,
    instruction::{AccountMeta, Instruction},
    native_token::LAMPORTS_PER_SOL,
    pubkey::Pubkey,
    rent::Rent,
    signature::{read_keypair_file, Keypair},
    signer::Signer,
    system_instruction, system_program, system_transaction,
    transaction::{SanitizedTransaction, Transaction},
};
use solana_transaction_status::UiTransactionEncoding;
use tokio::{
    runtime::Builder,
    time::{interval, MissedTickBehavior},
};

#[derive(Parser)]
struct Hammer {
    #[clap(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Initializes program, mint, and funded chad1/chad2 accounts
    Initialize,

    /// Performs the hammer operation
    Hammer {
        /// Approximate tps
        #[clap(long, default_value_t = 1_000)]
        tps: u64,

        /// Duration to hammer in seconds
        #[clap(long, default_value_t = 10)]
        time: u64,

        #[clap(long, default_value_t = 1)]
        num_pairs: usize,
    },

    /// Single Transfer
    Single,

    /// const of transfer
    TransferCost,
}

fn main() -> Result<(), Box<dyn Error>> {
    const RPC_ENDPOINT: &'static str = "http://localhost:8899";
    const PS_ENDPOINT: &'static str = "ws://localhost:8900/";

    // Builder::new_current_thread()
    Builder::new_multi_thread()
        .worker_threads(8)
        .enable_io()
        .enable_time()
        .global_queue_interval(32)
        .event_interval(16)
        .build()
        .unwrap()
        .block_on(async {
            let args = Hammer::parse();

            // Initialize client with payer
            let client = RpcClient::new_with_timeout_and_commitment(
                RPC_ENDPOINT.into(),
                Duration::from_secs(6),
                CommitmentConfig::confirmed(),
            );

            // Read config account, mint account, payer keypairs
            let cargo_manifest_path =
                PathBuf::from(&env::var("CARGO_MANIFEST_DIR")?);
            let cargo_workspace_path = cargo_manifest_path.parent().unwrap();
            let config_keypair =
                read_keypair_file(cargo_workspace_path.join("config.json"))?;
            let payer: &_ = Box::leak(Box::new(read_keypair_file(
                cargo_workspace_path.join("payer.json"),
            )?));
            let mint_keypair =
                read_keypair_file(cargo_manifest_path.join("atomic.json"))?;
            let chad1: &_ = Box::leak(Box::new(read_keypair_file(
                cargo_manifest_path.join("chad1.json"),
            )?));
            let chad2: &_ = Box::leak(Box::new(read_keypair_file(
                cargo_manifest_path.join("chad2.json"),
            )?));
            println!("loaded keypairs");


            match args.command {
                Commands::Initialize => {
                    let config = config_keypair.pubkey();
                    let create_config = system_transaction::create_account(
                        &payer,
                        &config_keypair,
                        client.get_latest_blockhash().await?,
                        Rent::default().minimum_balance(ProgramConfig::space()),
                        ProgramConfig::space() as u64,
                        &nanotoken::ID,
                    );
                    client
                        .send_and_confirm_transaction(&create_config)
                        .await
                        .unwrap();
                    println!("system_transaction::create_account config");

                    // Initialize mint
                    let create_mint = system_transaction::create_account(
                        &payer,
                        &mint_keypair,
                        client.get_latest_blockhash().await?,
                        Rent::default().minimum_balance(Mint::space()),
                        Mint::space() as u64,
                        &nanotoken::ID,
                    );
                    client
                        .send_and_confirm_transaction(&create_mint)
                        .await
                        .unwrap();
                    println!("system_transaction::create_account mint");

                    // Initialize config and mint
                    let ix_data = (Tag::InitializeConfig as u64)
                        .to_le_bytes()
                        .to_vec();

                    let accounts = vec![
                        // init config
                        AccountMeta::new(config, false),
                        AccountMeta::new_readonly(system_program::ID, false),
                        AccountMeta::new_readonly(payer.pubkey(), false),
                    ];
                    let instruction = Instruction {
                        program_id: nanotoken::ID,
                        accounts,
                        data: ix_data,
                    };
                    let transaction = Transaction::new_signed_with_payer(
                        &[instruction],
                        Some(&payer.pubkey()),
                        &[&payer],
                        client.get_latest_blockhash().await?,
                    );
                    client
                        .send_and_confirm_transaction(&transaction)
                        .await
                        .unwrap();
                    println!("initialized program and mint");

                    // Initialize mint
                    // Mint to
                    let mut ix_data = Vec::with_capacity(
                        8 + InitializeMintArgs::size()
                            + 2 * (8 + InitializeAccountArgs::size())
                            + (8 + MintArgs::size()),
                );

                    // Initialize mint
                    ix_data.extend_from_slice(
                        &(Tag::InitializeMint as u64).to_le_bytes(),
                    );
                    ix_data.extend_from_slice(payer.pubkey().as_ref());
                    ix_data.extend_from_slice(&[0; 8]); // decimals

                    let accounts = vec![
                        // init mint
                        AccountMeta::new(mint_keypair.pubkey(), false),
                        // remainder
                        AccountMeta::new(config, false),
                        AccountMeta::new_readonly(system_program::ID, false),
                        AccountMeta::new(payer.pubkey(), true),
                    ];
                    let instruction = Instruction {
                        program_id: nanotoken::ID,
                        accounts,
                        data: ix_data,
                    };
                    let transaction = Transaction::new_signed_with_payer(
                        &[instruction],
                        Some(&payer.pubkey()),
                        &[&payer],
                        client.get_latest_blockhash().await?,
                    );

                    client
                        .send_and_confirm_transaction(&transaction)
                        .await?;
                    println!("initialized mint");


                    // Initialize chad1 and chad2 token accounts
                    let (chad1_ta, chad1_ta_bump) =
                        TokenAccount::address(0, &chad1.pubkey());
                    let (chad2_ta, chad2_ta_bump) =
                        TokenAccount::address(0, &chad2.pubkey());


                    let mut ix_data = vec![];
                    {
                        // Initialize chad 1 ta
                        ix_data.extend_from_slice(
                            &(Tag::InitializeAccount as u64).to_le_bytes(),
                        );
                        ix_data.extend_from_slice(bytemuck::bytes_of(
                            &InitializeAccountArgs {
                                owner: chad1.pubkey(),
                                mint: 0,
                                bump: chad1_ta_bump as u64,
                            },
                        ));

                        // Initialize chad 2 ta
                        ix_data.extend_from_slice(
                            &(Tag::InitializeAccount as u64).to_le_bytes(),
                        );
                        ix_data.extend_from_slice(bytemuck::bytes_of(
                            &InitializeAccountArgs {
                                owner: chad2.pubkey(),
                                mint: 0,
                                bump: chad2_ta_bump as u64,
                            },
                        ));

                        // Mint to chad 1
                        ix_data.extend_from_slice(
                            &(Tag::Mint as u64).to_le_bytes(),
                        );
                        ix_data.extend_from_slice(bytemuck::bytes_of(
                            &MintArgs {
                                amount: 1_000_000_000,
                            },
                        ));
                    }
                    let accounts = vec![
                        // create
                        AccountMeta::new(chad1_ta, false),
                        // create
                        AccountMeta::new(chad2_ta, false),
                        // mint: to, mint, auth
                        AccountMeta::new(chad1_ta, false),
                        AccountMeta::new(mint_keypair.pubkey(), false),
                        AccountMeta::new_readonly(payer.pubkey(), true),
                        // remainder
                        AccountMeta::new(config_keypair.pubkey(), false),
                        AccountMeta::new_readonly(system_program::ID, false),
                        AccountMeta::new(payer.pubkey(), true),
                    ];
                    let instruction = Instruction {
                        program_id: nanotoken::ID,
                        accounts,
                        data: ix_data,
                    };
                    let transaction = Transaction::new_signed_with_payer(
                        &[instruction],
                        Some(&payer.pubkey()),
                        &[&payer],
                        client.get_latest_blockhash().await?,
                    );

                    client
                        .send_and_confirm_transaction(&transaction)
                        .await?;
                    println!("funded users");


                }
                Commands::Hammer { tps, time, num_pairs } => {
                    struct User {
                        kp: &'static Keypair,
                        ta: Pubkey,
                    }

                    let pairs = if num_pairs > 1 {
                        let mut pairs = vec![];
                        for p in 0..num_pairs {

                            let user1 = Box::leak(Box::new(Keypair::new()));
                            let user2 = Box::leak(Box::new(Keypair::new()));

                            let (user1_ta, user1_ta_bump) =
                                TokenAccount::address(0, &user1.pubkey());
                            let (user2_ta, user2_ta_bump) =
                                TokenAccount::address(0, &user2.pubkey());


                                let mut ix_data = vec![];
                                {
                                    // Initialize user 1 ta
                                    ix_data.extend_from_slice(
                                        &(Tag::InitializeAccount as u64).to_le_bytes(),
                                    );
                                    ix_data.extend_from_slice(bytemuck::bytes_of(
                                        &InitializeAccountArgs {
                                            owner: user1.pubkey(),
                                            mint: 0,
                                            bump: user1_ta_bump as u64,
                                        },
                                    ));

                                    // Initialize user 2 ta
                                    ix_data.extend_from_slice(
                                        &(Tag::InitializeAccount as u64).to_le_bytes(),
                                    );
                                    ix_data.extend_from_slice(bytemuck::bytes_of(
                                        &InitializeAccountArgs {
                                            owner: user2.pubkey(),
                                            mint: 0,
                                            bump: user2_ta_bump as u64,
                                        },
                                    ));
                                    // Mint to user 1
                                    ix_data.extend_from_slice(
                                        &(Tag::Mint as u64).to_le_bytes(),
                                    );
                                    ix_data.extend_from_slice(bytemuck::bytes_of(
                                        &MintArgs {
                                            amount: 1_000_000_000,
                                        },
                                    ));
                                }

                            // Initialize user1 and user2 token accounts
                                let accounts = vec![
                                    // create
                                    AccountMeta::new(user1_ta, false),
                                    // create
                                    AccountMeta::new(user2_ta, false),
                                    // mint: to, mint, auth
                                    AccountMeta::new(user1_ta, false),
                                    AccountMeta::new(mint_keypair.pubkey(), false),
                                    AccountMeta::new_readonly(payer.pubkey(), true),
                                    // remainder
                                    AccountMeta::new(config_keypair.pubkey(), false),
                                    AccountMeta::new_readonly(system_program::ID, false),
                                    AccountMeta::new(payer.pubkey(), true),
                                ];
                                let instruction = Instruction {
                                    program_id: nanotoken::ID,
                                    accounts,
                                    data: ix_data,
                                };
                                let transaction = Transaction::new_signed_with_payer(
                                    &[instruction],
                                    Some(&payer.pubkey()),
                                    &[&payer],
                                    client.get_latest_blockhash().await?,
                                );

                                client
                                    .send_and_confirm_transaction(&transaction)
                                    .await?;
                                println!(
                                    "many: initialized pair {p} user1 and user2 token accounts and gigaminted."
                                );

                            pairs.push([
                                User {
                                    kp: user1,
                                    ta: user1_ta
                                },
                                User {
                                    kp: user2,
                                    ta: user2_ta
                                },
                            ]);
                        }
                        pairs
                    } else {
                        let (chad1_ta, _chad1_ta_bump) =
                        TokenAccount::address(0, &chad1.pubkey());
                    let (chad2_ta, _chad2_ta_bump) =
                        TokenAccount::address(0, &chad2.pubkey());
                        vec![[User{ kp: chad1, ta: chad1_ta}, User{kp:chad2, ta: chad2_ta}]]
                    };
                    let pairs = Vec::leak(pairs);

                    // Fund users
                    for pair in &*pairs {
                        let chad1 = pair[0].kp;
                        let chad2 = pair[1].kp;
                        let fund_1_ix = system_instruction::transfer(&payer.pubkey(), &chad1.pubkey(), LAMPORTS_PER_SOL / 2);
                        let fund_2_ix = system_instruction::transfer(&payer.pubkey(), &chad2.pubkey(), LAMPORTS_PER_SOL / 2);

                        let fund_1_and_2_tx = Transaction::new_signed_with_payer(
                            &[fund_1_ix, fund_2_ix],
                            Some(&payer.pubkey()),
                            &[&payer],
                            client.get_latest_blockhash().await?
                        );
                        client.send_and_confirm_transaction(&fund_1_and_2_tx).await?;
                    }

                    let interval_nanos = 1_000_000_000 / tps;
                    let mut interval =
                        interval(Duration::from_nanos(1_000_000_000 / tps));
                    interval
                        .set_missed_tick_behavior(MissedTickBehavior::Burst);
                    println!(
                        "set interval_nanos at {interval_nanos} -> tps ≈ {}",
                        1_000_000_000 / interval_nanos
                    );

                    let timer = Instant::now();
                    let pb = ProgressBar::new(time)
                .with_style(ProgressStyle::with_template(
                "[{elapsed_precise}] {bar:40.cyan/blue} {pos:>7}/{len:7} {msg}",
            )?);
                    static SENT: AtomicU64 = AtomicU64::new(0);
                    static FAILED: AtomicU64 = AtomicU64::new(0);

                    let blockhash: &RwLock<_> = Box::leak(Box::new(RwLock::new(client.get_latest_blockhash().await?)));
                    let mut idx: u32 = 0;

                    // Switch to tpu
                    let client: &'static TpuClient<_, _, _> =
                        Box::leak(Box::new(
                            TpuClient::new(
                                "chad",
                                Arc::new(client),
                                PS_ENDPOINT,
                                TpuClientConfig { fanout_slots: 3 },
                            )
                            .await?,
                        ));

                    // every 8 seconds
                    let fetch_every: u32 = 8 * tps as u32;

                    'send_loop: for iteration in 0.. {
                        interval.tick().await;

                        if idx % fetch_every == fetch_every - 1 {
                            tokio::task::spawn(async move {
                                let current = *blockhash.read().unwrap();
                            if let Ok(bh) = client
                            .rpc_client()
                            .get_new_latest_blockhash(&current).await {
                                *blockhash.write().unwrap() = bh;
                            }});
                        }

                        let chad1 = pairs[iteration%num_pairs][0].kp;
                        let chad2 = pairs[iteration%num_pairs][1].kp;
                        let chad1_ta = pairs[iteration%num_pairs][0].ta;
                        let chad2_ta = pairs[iteration%num_pairs][1].ta;

                        tokio::task::spawn(async move {
                            let num_transfers = 2;
                            let mut ix_data =
                                vec![
                                    0;
                                    num_transfers * (8 + TransferArgs::size())
                                ];
                            let mut accounts = vec![];
                            for n in 0..num_transfers {
                                let disc_offset =
                                    8 * n + n * TransferArgs::size();
                                ix_data[disc_offset..8 + disc_offset]
                                    .copy_from_slice(
                                        &(Tag::Transfer as u64).to_le_bytes(),
                                    );
                                let TransferArgs { amount } =
                                    bytemuck::try_from_bytes_mut(
                                        &mut ix_data[disc_offset + 8
                                            ..disc_offset
                                                + 8
                                                + TransferArgs::size()],
                                    )
                                    .unwrap();
                                *amount = 1;

                                if n % 2 == 0 {
                                    accounts.extend([
                                        AccountMeta::new(chad1_ta, false),
                                        AccountMeta::new(chad2_ta, false),
                                        AccountMeta::new_readonly(
                                            chad1.pubkey(),
                                            true,
                                        ),
                                    ])
                                } else {
                                    accounts.extend([
                                        AccountMeta::new(chad2_ta, false),
                                        AccountMeta::new(chad1_ta, false),
                                        AccountMeta::new_readonly(
                                            chad2.pubkey(),
                                            true,
                                        ),
                                    ])
                                }
                            }
                            let request_cus =
                            ComputeBudgetInstruction::set_compute_unit_limit(
                                800,
                            );
                            // this acts as nonce
                            let ix_account_size = ComputeBudgetInstruction::set_loaded_accounts_data_size_limit(56 * 1024 + (idx % (fetch_every)));
                            // let noop_nonce_ix = Instruction {
                            //     program_id: noop_program::ID.into(),
                            //     accounts: vec![],
                            //     data: idx.to_le_bytes().to_vec(),
                            // };

                            let instruction = Instruction {
                                program_id: nanotoken::ID,
                                accounts,
                                data: ix_data,
                            };
                            let transaction =
                                Transaction::new_signed_with_payer(
                                    &[
                                        request_cus, 
                                        ix_account_size, 
                                        // noop_nonce_ix, 
                                        instruction
                                    ],
                                    Some(&chad1.pubkey()),
                                    &[&chad1, &chad2],
                                    *blockhash.read().unwrap(),
                                );

                            // match client
                            //     .rpc_client()
                            //     // .send_and_confirm_transaction(&transaction)
                            //     .send_transaction_with_config(&transaction, RpcSendTransactionConfig {
                            //         skip_preflight: true,
                            //         ..Default::default()
                            //     })
                            //     .await
                            // {
                            //     Ok(_) => {
                            //         SENT.fetch_add(1, Ordering::Relaxed);
                            //     }
                            //     Err(e) => {
                            //         if FAILED.fetch_add(1, Ordering::Relaxed) % 10000 == 0 {
                            //             println!("{e:#?}");
                            //         };
                            //     }
                            // }

                            match client
                                .try_send_transaction(&transaction)
                                .await
                            {
                                Ok(_) => {
                                    SENT.fetch_add(1, Ordering::Relaxed);
                                }
                                Err(_e) => {
                                    FAILED.fetch_add(1, Ordering::Relaxed);
                                }
                            }
                        });

                        // Update progress bar
                        let seconds_elapsed = timer.elapsed().as_secs();
                        pb.update(|pb_inner| {
                            pb_inner.set_pos(seconds_elapsed);
                        });
                        let sent_txs = SENT.load(Ordering::Relaxed);
                        pb.set_message(format!(
                            "{} sent txs ≈ {} tps; failed {}",
                            sent_txs,
                            sent_txs / seconds_elapsed.max(1),
                            FAILED.load(Ordering::Relaxed)
                        ));

                        idx += 1;

                        if timer.elapsed().as_secs() == time {
                            pb.finish();
                            break 'send_loop;
                        }
                    }
                }
                Commands::Single => {
                    let (chad1_ta, _chad1_ta_bump) =
                        TokenAccount::address(0, &chad1.pubkey());
                    let (chad2_ta, _chad2_ta_bump) =
                        TokenAccount::address(0, &chad2.pubkey());
                    let num_transfers = 1;
                    let mut ix_data =
                        vec![0; num_transfers * (8 + TransferArgs::size())];
                    let mut accounts = vec![];
                    ix_data[0..8].copy_from_slice(
                        &(Tag::Transfer as u64).to_le_bytes(),
                    );
                    let TransferArgs { amount } =
                        bytemuck::try_from_bytes_mut(
                            &mut ix_data[8
                                ..8 + TransferArgs::size()],
                        )
                        .unwrap();
                    *amount = 1;

                    accounts.extend([
                        AccountMeta::new(chad1_ta, false),
                        AccountMeta::new(chad2_ta, false),
                        AccountMeta::new_readonly(chad1.pubkey(), true),
                    ]);

                    let request_cus =
                        ComputeBudgetInstruction::set_compute_unit_limit(600);
                    let ix_account_size = ComputeBudgetInstruction::set_loaded_accounts_data_size_limit(64 * 1024);

                    let instruction = Instruction {
                        program_id: nanotoken::ID,
                        accounts,
                        data: ix_data,
                    };
                    let transaction = Transaction::new_signed_with_payer(
                        &[request_cus, ix_account_size, instruction],
                        Some(&chad1.pubkey()),
                        &[&chad1],
                        client.get_latest_blockhash().await?,
                    );
                    let sig = client.send_and_confirm_transaction(&transaction).await?;
                    println!("{:#?}", client.get_transaction(&sig, UiTransactionEncoding::Binary).await?)
                }
                Commands::TransferCost => {
                    let (chad1_ta, _chad1_ta_bump) =
                        TokenAccount::address(0, &chad1.pubkey());
                    let (chad2_ta, _chad2_ta_bump) =
                        TokenAccount::address(0, &chad2.pubkey());
                    let num_transfers = 2;
                    let mut ix_data =
                        vec![0; num_transfers * (8 + TransferArgs::size())];
                    let mut accounts = vec![];
                    for n in 0..num_transfers {
                        let disc_offset = 8 * n + n * TransferArgs::size();
                        ix_data[disc_offset..8 + disc_offset].copy_from_slice(
                            &(Tag::Transfer as u64).to_le_bytes(),
                        );
                        let TransferArgs { amount } =
                            bytemuck::try_from_bytes_mut(
                                &mut ix_data[disc_offset + 8
                                    ..disc_offset + 8 + TransferArgs::size()],
                            )
                            .unwrap();
                        *amount = 1;

                        if n % 2 == 0 {
                            accounts.extend([
                                AccountMeta::new(chad1_ta, false),
                                AccountMeta::new(chad2_ta, false),
                                AccountMeta::new_readonly(chad1.pubkey(), true),
                            ])
                        } else {
                            accounts.extend([
                                AccountMeta::new(chad2_ta, false),
                                AccountMeta::new(chad1_ta, false),
                                AccountMeta::new_readonly(chad2.pubkey(), true),
                            ])
                        }
                    }
                    let request_cus =
                        ComputeBudgetInstruction::set_compute_unit_limit(650);
                    let ix_account_size = ComputeBudgetInstruction::set_loaded_accounts_data_size_limit(64 * 1024);
                    let noop_nonce_ix = Instruction {
                        program_id: noop_program::ID.into(),
                        accounts: vec![],
                        data: 0u64.to_le_bytes().to_vec(),
                    };

                    let instruction = Instruction {
                        program_id: nanotoken::ID,
                        accounts,
                        data: ix_data,
                    };
                    let transaction = Transaction::new_signed_with_payer(
                        &[request_cus, ix_account_size, noop_nonce_ix, instruction],
                        Some(&chad1.pubkey()),
                        &[&chad1, &chad2],
                        client.get_latest_blockhash().await?,
                    );

                    let cost = solana_cost_model::cost_model::CostModel::calculate_cost(
                        &SanitizedTransaction::try_from_legacy_transaction(
                            transaction,
                        )?,
                        // &Default::default(),
                        &FeatureSet::all_enabled(),
                    );
                    println!("cost = {cost:?}");
                    println!("cost sum = {}", cost.sum());

                    let mut tracker = CostTracker::default();
                    while tracker.try_add(&cost).is_ok() {}
                    println!("{tracker:#?}");

                    // single

                let num_transfers = 1;
                let mut ix_data =
                    vec![0; num_transfers * (8 + TransferArgs::size())];
                let mut accounts = vec![];
                for n in 0..num_transfers {
                    let disc_offset = 8 * n + n * TransferArgs::size();
                    ix_data[disc_offset..8 + disc_offset].copy_from_slice(
                        &(Tag::Transfer as u64).to_le_bytes(),
                    );
                    let TransferArgs { amount } =
                        bytemuck::try_from_bytes_mut(
                            &mut ix_data[disc_offset + 8
                                ..disc_offset + 8 + TransferArgs::size()],
                        )
                        .unwrap();
                    *amount = 1;

                        accounts.extend([
                            AccountMeta::new(chad1_ta, false),
                            AccountMeta::new(chad2_ta, false),
                            AccountMeta::new_readonly(chad1.pubkey(), true),
                        ])
                }
                let request_cus =
                    ComputeBudgetInstruction::set_compute_unit_limit(650);
                    let ix_account_size = ComputeBudgetInstruction::set_loaded_accounts_data_size_limit(64 * 1024);
                    let noop_nonce_ix = Instruction {
                    program_id: noop_program::ID.into(),
                    accounts: vec![],
                    data: 0u64.to_le_bytes().to_vec(),
                };

                let instruction = Instruction {
                    program_id: nanotoken::ID,
                    accounts,
                    data: ix_data,
                };
                let transaction = Transaction::new_signed_with_payer(
                    &[request_cus,ix_account_size, noop_nonce_ix, instruction],
                    Some(&chad1.pubkey()),
                    &[&chad1],
                    client.get_latest_blockhash().await?,
                );

                let cost = solana_cost_model::cost_model::CostModel::calculate_cost(
                    &SanitizedTransaction::try_from_legacy_transaction(
                        transaction,
                    )?,
                    // &Default::default(),
                    &FeatureSet::all_enabled(),
                );
                println!("cost = {cost:?}");
                println!("cost sum = {}", cost.sum());

                let mut tracker = CostTracker::default();
                // tracker.set_limits(
                //     solana_cost_model::block_cost_limits::MAX_WRITABLE_ACCOUNT_UNITS.saturating_div(5 as u64),
                //     solana_cost_model::block_cost_limits::MAX_BLOCK_UNITS.saturating_div(5 as u64),
                //     solana_cost_model::block_cost_limits::MAX_VOTE_UNITS.saturating_div(5 as u64),
                // );
                while tracker.try_add(&cost).is_ok() {}
                println!("{tracker:#?}");
                }
            };

            tokio::time::sleep(Duration::from_millis(5000)).await;

            Ok::<(), Box<dyn Error>>(())
        })?;

    Ok(())
}
