use criterion::{criterion_group, criterion_main, Criterion};
use dxr::Value;
use ros_core_rs::param_tree::ParamTree;
use std::hint::black_box;
use std::sync::Arc;
use tokio::runtime::Runtime;

fn create_test_tree() -> Arc<ParamTree> {
    let tree = Arc::new(ParamTree::new());

    // Initialize with some test data
    let runtime = Runtime::new().unwrap();
    runtime.block_on(async {
        tree.set("robot_id", Value::i4(42).into(), "test".to_string())
            .await
            .unwrap();
        tree.set("robot_speed", Value::double(3.0).into(), "test".to_string())
            .await
            .unwrap();
        tree.set(
            "arms/left/length",
            Value::double(0.5).into(),
            "test".to_string(),
        )
        .await
        .unwrap();
        tree.set(
            "arms/right/length",
            Value::double(0.5).into(),
            "test".to_string(),
        )
        .await
        .unwrap();
        tree.set(
            "sensors/camera/resolution",
            Value::i4(1920).into(),
            "test".to_string(),
        )
        .await
        .unwrap();
    });

    tree
}

fn benchmark_concurrent_reads(c: &mut Criterion) {
    let mut group = c.benchmark_group("concurrent_reads");
    let runtime = Runtime::new().unwrap();

    group.bench_function("single_reader", |b| {
        let tree = create_test_tree();
        b.iter(|| {
            runtime.block_on(async {
                black_box(tree.get("robot_id").await);
                black_box(tree.get("arms/left/length").await);
                black_box(tree.contains("sensors/camera/resolution").await);
            });
        });
    });

    group.bench_function("multiple_readers_sequential", |b| {
        let tree = create_test_tree();
        b.iter(|| {
            runtime.block_on(async {
                for _ in 0..10 {
                    black_box(tree.get("robot_id").await);
                    black_box(tree.get("arms/left/length").await);
                    black_box(tree.contains("sensors/camera/resolution").await);
                }
            });
        });
    });

    group.bench_function("multiple_readers_concurrent", |b| {
        let tree = create_test_tree();
        b.iter(|| {
            runtime.block_on(async {
                let handles: Vec<_> = (0..10)
                    .map(|_| {
                        let tree = tree.clone();
                        tokio::spawn(async move {
                            black_box(tree.get("robot_id").await);
                            black_box(tree.get("arms/left/length").await);
                            black_box(tree.contains("sensors/camera/resolution").await);
                        })
                    })
                    .collect();

                for handle in handles {
                    handle.await.unwrap();
                }
            });
        });
    });

    group.finish();
}

fn benchmark_read_write_contention(c: &mut Criterion) {
    let mut group = c.benchmark_group("read_write_contention");
    let runtime = Runtime::new().unwrap();

    group.bench_function("read_with_occasional_write", |b| {
        let tree = create_test_tree();
        b.iter(|| {
            runtime.block_on(async {
                // Spawn multiple readers
                let read_handles: Vec<_> = (0..5)
                    .map(|_| {
                        let tree = tree.clone();
                        tokio::spawn(async move {
                            for _ in 0..20 {
                                black_box(tree.get("robot_id").await);
                                black_box(tree.get("arms/left/length").await);
                            }
                        })
                    })
                    .collect();

                // Spawn one writer
                let write_handle = {
                    let tree = tree.clone();
                    tokio::spawn(async move {
                        for i in 0..5 {
                            tree.set("robot_id", Value::i4(i).into(), "test".to_string())
                                .await
                                .unwrap();
                        }
                    })
                };

                // Wait for all to complete
                for handle in read_handles {
                    handle.await.unwrap();
                }
                write_handle.await.unwrap();
            });
        });
    });

    group.bench_function("heavy_write_with_reads", |b| {
        let tree = create_test_tree();
        b.iter(|| {
            runtime.block_on(async {
                // Spawn readers
                let read_handles: Vec<_> = (0..3)
                    .map(|_| {
                        let tree = tree.clone();
                        tokio::spawn(async move {
                            for _ in 0..10 {
                                black_box(tree.get("robot_id").await);
                            }
                        })
                    })
                    .collect();

                // Spawn multiple writers
                let write_handles: Vec<_> = (0..3)
                    .map(|i| {
                        let tree = tree.clone();
                        tokio::spawn(async move {
                            for j in 0..10 {
                                tree.set(
                                    &format!("param_{}", i),
                                    Value::i4(j).into(),
                                    "test".to_string(),
                                )
                                .await
                                .unwrap();
                            }
                        })
                    })
                    .collect();

                // Wait for all to complete
                for handle in read_handles {
                    handle.await.unwrap();
                }
                for handle in write_handles {
                    handle.await.unwrap();
                }
            });
        });
    });

    group.finish();
}

fn benchmark_subscription_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("subscription_operations");
    let runtime = Runtime::new().unwrap();

    group.bench_function("subscribe_unsubscribe", |b| {
        let tree = create_test_tree();
        b.iter(|| {
            runtime.block_on(async {
                for i in 0..10 {
                    let _value = tree
                        .subscribe(
                            format!("node_{}", i),
                            "/".to_string(),
                            format!("http://node_{}", i),
                        )
                        .await;

                    tree.unsubscribe(format!("http://node_{}", i), "/".to_string())
                        .await;
                }
            });
        });
    });

    group.bench_function("subscription_with_updates", |b| {
        let tree = create_test_tree();
        b.iter(|| {
            runtime.block_on(async {
                // Subscribe to parameters
                let sub_handles: Vec<_> = (0..5)
                    .map(|i| {
                        let tree = tree.clone();
                        tokio::spawn(async move {
                            let _value = tree
                                .subscribe(
                                    format!("node_{}", i),
                                    "/".to_string(),
                                    format!("http://node_{}", i),
                                )
                                .await;
                        })
                    })
                    .collect();

                // Wait for subscriptions to complete
                for handle in sub_handles {
                    handle.await.unwrap();
                }

                // Update parameters (triggers subscription notifications)
                let update_handles: Vec<_> = (0..10)
                    .map(|i| {
                        let tree = tree.clone();
                        tokio::spawn(async move {
                            tree.set(
                                &format!("param_{}", i),
                                Value::i4(i).into(),
                                "test".to_string(),
                            )
                            .await
                            .unwrap();
                        })
                    })
                    .collect();

                // Wait for updates to complete
                for handle in update_handles {
                    handle.await.unwrap();
                }
            });
        });
    });

    group.finish();
}

fn benchmark_mixed_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("mixed_operations");
    let runtime = Runtime::new().unwrap();

    group.bench_function("realistic_workload", |b| {
        let tree = create_test_tree();
        b.iter(|| {
            runtime.block_on(async {
                // Subscribe to parameters
                let sub_handles: Vec<_> = (0..3)
                    .map(|i| {
                        let tree = tree.clone();
                        tokio::spawn(async move {
                            let _value = tree
                                .subscribe(
                                    format!("node_{}", i),
                                    "/".to_string(),
                                    format!("http://node_{}", i),
                                )
                                .await;
                        })
                    })
                    .collect();

                // Wait for subscriptions
                for handle in sub_handles {
                    handle.await.unwrap();
                }

                // Mixed read/write operations
                let operation_handles: Vec<_> = (0..5)
                    .map(|i| {
                        let tree = tree.clone();
                        tokio::spawn(async move {
                            for j in 0..5 {
                                // Read operations
                                black_box(tree.get("robot_id").await);
                                black_box(tree.contains("arms/left/length").await);

                                // Write operations
                                tree.set(
                                    &format!("param_{}_{}", i, j),
                                    Value::i4(j).into(),
                                    "test".to_string(),
                                )
                                .await
                                .unwrap();
                            }
                        })
                    })
                    .collect();

                // Wait for all operations
                for handle in operation_handles {
                    handle.await.unwrap();
                }
            });
        });
    });

    group.finish();
}

fn benchmark_lock_contention_scenarios(c: &mut Criterion) {
    let mut group = c.benchmark_group("lock_contention");
    let runtime = Runtime::new().unwrap();

    group.bench_function("many_readers_few_writers", |b| {
        let tree = create_test_tree();
        b.iter(|| {
            runtime.block_on(async {
                // Many readers
                let read_handles: Vec<_> = (0..20)
                    .map(|_| {
                        let tree = tree.clone();
                        tokio::spawn(async move {
                            for _ in 0..5 {
                                black_box(tree.get("robot_id").await);
                                black_box(tree.get("arms/left/length").await);
                            }
                        })
                    })
                    .collect();

                // Few writers
                let write_handles: Vec<_> = (0..2)
                    .map(|i| {
                        let tree = tree.clone();
                        tokio::spawn(async move {
                            for j in 0..3 {
                                tree.set(
                                    &format!("writer_{}_{}", i, j),
                                    Value::i4(j).into(),
                                    "test".to_string(),
                                )
                                .await
                                .unwrap();
                            }
                        })
                    })
                    .collect();

                // Wait for all
                for handle in read_handles {
                    handle.await.unwrap();
                }
                for handle in write_handles {
                    handle.await.unwrap();
                }
            });
        });
    });

    group.bench_function("burst_writes", |b| {
        let tree = create_test_tree();
        b.iter(|| {
            runtime.block_on(async {
                // Burst of writes
                let write_handles: Vec<_> = (0..10)
                    .map(|i| {
                        let tree = tree.clone();
                        tokio::spawn(async move {
                            for j in 0..5 {
                                tree.set(
                                    &format!("burst_{}_{}", i, j),
                                    Value::i4(j).into(),
                                    "test".to_string(),
                                )
                                .await
                                .unwrap();
                            }
                        })
                    })
                    .collect();

                // Some readers during burst
                let read_handles: Vec<_> = (0..5)
                    .map(|_| {
                        let tree = tree.clone();
                        tokio::spawn(async move {
                            for _ in 0..10 {
                                black_box(tree.get("robot_id").await);
                            }
                        })
                    })
                    .collect();

                // Wait for all
                for handle in write_handles {
                    handle.await.unwrap();
                }
                for handle in read_handles {
                    handle.await.unwrap();
                }
            });
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    benchmark_concurrent_reads,
    benchmark_read_write_contention,
    benchmark_subscription_operations,
    benchmark_mixed_operations,
    benchmark_lock_contention_scenarios
);
criterion_main!(benches);
