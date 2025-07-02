use criterion::{criterion_group, criterion_main, Criterion};
use dxr::{TryToValue, Value};
use maplit::hashmap;
use ros_core_rs::ParamValue;
use std::hint::black_box;

fn create_simple_tree() -> ParamValue {
    ParamValue::HashMap(hashmap! {
        "run_id".to_owned() => ParamValue::Value(Value::string("asdf-jkl0".to_owned())),
        "robot_id".to_owned() => ParamValue::Value(Value::i4(42)),
    })
}

fn create_complex_tree() -> ParamValue {
    ParamValue::HashMap(hashmap! {
        "run_id".to_owned() => ParamValue::Value(Value::string("asdf-jkl0".to_owned())),
        "robot_id".to_owned() => ParamValue::Value(Value::i4(42)),
        "robot_configs".to_owned() => ParamValue::Array(vec![
            ParamValue::HashMap(hashmap! {
                "robot_speed".to_owned() => ParamValue::Value(Value::double(3.0)),
                "robot_id".to_owned() => ParamValue::Value(Value::i4(24))
            })
        ]),
        "arms".to_owned() => ParamValue::HashMap(hashmap! {
            "arm_left".to_owned() => ParamValue::HashMap(hashmap! {
                "length".to_owned() => ParamValue::Value(Value::double(-0.45)),
                "joints".to_owned() => ParamValue::HashMap(hashmap! {
                    "shoulder".to_owned() => ParamValue::Value(Value::double(0.0)),
                    "elbow".to_owned() => ParamValue::Value(Value::double(90.0)),
                    "wrist".to_owned() => ParamValue::Value(Value::double(45.0))
                })
            }),
            "arm_right".to_owned() => ParamValue::HashMap(hashmap! {
                "length".to_owned() => ParamValue::Value(Value::double(0.45)),
                "status".to_owned() => ParamValue::Value(Value::string("active".to_owned()))
            })
        }),
        "sensors".to_owned() => ParamValue::HashMap(hashmap! {
            "camera".to_owned() => ParamValue::HashMap(hashmap! {
                "resolution".to_owned() => ParamValue::Array(vec![
                    ParamValue::Value(Value::i4(1920)),
                    ParamValue::Value(Value::i4(1080))
                ]),
                "fps".to_owned() => ParamValue::Value(Value::i4(30))
            }),
            "lidar".to_owned() => ParamValue::HashMap(hashmap! {
                "range".to_owned() => ParamValue::Value(Value::double(100.0)),
                "frequency".to_owned() => ParamValue::Value(Value::double(10.0))
            })
        })
    })
}

fn benchmark_contains(c: &mut Criterion) {
    let mut group = c.benchmark_group("contains");

    let simple_tree = create_simple_tree();
    let complex_tree = create_complex_tree();

    group.bench_function("simple_tree_existing", |b| {
        b.iter(|| {
            black_box(simple_tree.contains(black_box("run_id")));
        })
    });

    group.bench_function("simple_tree_missing", |b| {
        b.iter(|| {
            black_box(simple_tree.contains(black_box("missing_key")));
        })
    });

    group.bench_function("complex_tree_shallow", |b| {
        b.iter(|| {
            black_box(complex_tree.contains(black_box("robot_id")));
        })
    });

    group.bench_function("complex_tree_deep", |b| {
        b.iter(|| {
            black_box(complex_tree.contains(black_box("arms/arm_left/joints/shoulder")));
        })
    });

    group.bench_function("complex_tree_missing", |b| {
        b.iter(|| {
            black_box(complex_tree.contains(black_box("arms/arm_left/joints/missing")));
        })
    });

    group.finish();
}

fn benchmark_get(c: &mut Criterion) {
    let mut group = c.benchmark_group("get");

    let simple_tree = create_simple_tree();
    let complex_tree = create_complex_tree();

    group.bench_function("simple_tree_existing", |b| {
        b.iter(|| {
            black_box(simple_tree.get(black_box(["run_id"])));
        })
    });

    group.bench_function("simple_tree_missing", |b| {
        b.iter(|| {
            black_box(simple_tree.get(black_box(["missing_key"])));
        })
    });

    group.bench_function("complex_tree_shallow", |b| {
        b.iter(|| {
            black_box(complex_tree.get(black_box(["robot_id"])));
        })
    });

    group.bench_function("complex_tree_deep", |b| {
        b.iter(|| {
            black_box(complex_tree.get(black_box(["arms", "arm_left", "joints", "shoulder"])));
        })
    });

    group.bench_function("complex_tree_array", |b| {
        b.iter(|| {
            black_box(complex_tree.get(black_box(["sensors", "camera", "resolution"])));
        })
    });

    group.bench_function("complex_tree_missing", |b| {
        b.iter(|| {
            black_box(complex_tree.get(black_box(["arms", "arm_left", "joints", "missing"])));
        })
    });

    group.finish();
}

fn benchmark_get_keys(c: &mut Criterion) {
    let mut group = c.benchmark_group("get_keys");

    let simple_tree = create_simple_tree();
    let complex_tree = create_complex_tree();

    group.bench_function("simple_tree", |b| {
        b.iter(|| {
            black_box(simple_tree.get_keys());
        })
    });

    group.bench_function("complex_tree", |b| {
        b.iter(|| {
            black_box(complex_tree.get_keys());
        })
    });

    group.finish();
}

fn benchmark_remove(c: &mut Criterion) {
    let mut group = c.benchmark_group("remove");

    group.bench_function("simple_tree_shallow", |b| {
        b.iter(|| {
            let mut tree = create_simple_tree();
            tree.remove(black_box(["run_id"]));
            black_box(tree);
        })
    });

    group.bench_function("complex_tree_shallow", |b| {
        b.iter(|| {
            let mut tree = create_complex_tree();
            tree.remove(black_box(["robot_id"]));
            black_box(tree);
        })
    });

    group.bench_function("complex_tree_deep", |b| {
        b.iter(|| {
            let mut tree = create_complex_tree();
            tree.remove(black_box(["arms", "arm_left", "joints", "shoulder"]));
            black_box(tree);
        })
    });

    group.bench_function("complex_tree_missing", |b| {
        b.iter(|| {
            let mut tree = create_complex_tree();
            tree.remove(black_box(["arms", "arm_left", "joints", "missing"]));
            black_box(tree);
        })
    });

    group.finish();
}

fn benchmark_update_inner(c: &mut Criterion) {
    let mut group = c.benchmark_group("update_inner");

    group.bench_function("simple_tree_existing", |b| {
        b.iter(|| {
            let mut tree = create_simple_tree();
            tree.update_inner(
                black_box(["run_id"].iter()),
                black_box(Value::string("new_value".to_owned()).into()),
            );
            black_box(tree);
        })
    });

    group.bench_function("simple_tree_new", |b| {
        b.iter(|| {
            let mut tree = create_simple_tree();
            tree.update_inner(
                black_box(["new_key"].iter()),
                black_box(Value::i4(123).into()),
            );
            black_box(tree);
        })
    });

    group.bench_function("complex_tree_existing_deep", |b| {
        b.iter(|| {
            let mut tree = create_complex_tree();
            tree.update_inner(
                black_box(["arms", "arm_left", "joints", "shoulder"].iter()),
                black_box(Value::double(180.0).into()),
            );
            black_box(tree);
        })
    });

    group.bench_function("complex_tree_new_deep", |b| {
        b.iter(|| {
            let mut tree = create_complex_tree();
            tree.update_inner(
                black_box(["arms", "arm_left", "joints", "new_joint"].iter()),
                black_box(Value::double(45.0).into()),
            );
            black_box(tree);
        })
    });

    group.bench_function("complex_tree_new_path", |b| {
        b.iter(|| {
            let mut tree = create_complex_tree();
            tree.update_inner(
                black_box(["new_section", "new_subsection", "new_value"].iter()),
                black_box(Value::string("test".to_owned()).into()),
            );
            black_box(tree);
        })
    });

    group.finish();
}

fn benchmark_from_value(c: &mut Criterion) {
    let mut group = c.benchmark_group("from_value");

    let simple_value = hashmap! {
        "key1".to_owned() => Value::string("value1".to_owned()),
        "key2".to_owned() => Value::i4(42),
    }
    .try_to_value()
    .unwrap();

    let complex_value = hashmap! {
        "nested".to_owned() => hashmap! {
            "array".to_owned() => vec![
                Value::i4(1),
                Value::i4(2),
                Value::i4(3),
            ].try_to_value().unwrap(),
            "string".to_owned() => Value::string("test".to_owned()),
        }.try_to_value().unwrap(),
        "simple".to_owned() => Value::double(3.14),
    }
    .try_to_value()
    .unwrap();

    group.bench_function("simple_struct", |b| {
        b.iter(|| {
            black_box(ParamValue::from(black_box(&simple_value)));
        })
    });

    group.bench_function("complex_struct", |b| {
        b.iter(|| {
            black_box(ParamValue::from(black_box(&complex_value)));
        })
    });

    group.bench_function("simple_array", |b| {
        let array_value = vec![Value::i4(1), Value::i4(2), Value::i4(3)]
            .try_to_value()
            .unwrap();
        b.iter(|| {
            black_box(ParamValue::from(black_box(&array_value)));
        })
    });

    group.bench_function("simple_value", |b| {
        let simple = Value::string("test".to_owned());
        b.iter(|| {
            black_box(ParamValue::from(black_box(&simple)));
        })
    });

    group.finish();
}

fn benchmark_try_to_value(c: &mut Criterion) {
    let mut group = c.benchmark_group("try_to_value");

    let simple_tree = create_simple_tree();
    let complex_tree = create_complex_tree();

    group.bench_function("simple_tree", |b| {
        b.iter(|| {
            black_box(simple_tree.try_to_value().unwrap());
        })
    });

    group.bench_function("complex_tree", |b| {
        b.iter(|| {
            black_box(complex_tree.try_to_value().unwrap());
        })
    });

    group.finish();
}

fn benchmark_tree_operations_combined(c: &mut Criterion) {
    let mut group = c.benchmark_group("combined_operations");

    group.bench_function("read_write_cycle", |b| {
        b.iter(|| {
            let mut tree = create_complex_tree();

            // Read some values
            let _robot_id = tree.get(["robot_id"]);
            let _arm_length = tree.get(["arms", "arm_left", "length"]);

            // Update some values
            tree.update_inner(["robot_id"].iter(), Value::i4(100).into());
            tree.update_inner(
                ["arms", "arm_left", "length"].iter(),
                Value::double(0.5).into(),
            );

            // Check if values exist
            let _has_robot = tree.contains("robot_id");
            let _has_arm = tree.contains("arms/arm_left/length");

            black_box(tree);
        })
    });

    group.bench_function("deep_navigation", |b| {
        b.iter(|| {
            let tree = create_complex_tree();

            // Navigate through deep paths
            let _camera_res = tree.get(["sensors", "camera", "resolution"]);
            let _lidar_range = tree.get(["sensors", "lidar", "range"]);
            let _shoulder_joint = tree.get(["arms", "arm_left", "joints", "shoulder"]);
            let _robot_config = tree.get(["robot_configs"]);

            black_box(tree);
        })
    });

    group.finish();
}

criterion_group!(
    benches,
    benchmark_contains,
    benchmark_get,
    benchmark_get_keys,
    benchmark_remove,
    benchmark_update_inner,
    benchmark_from_value,
    benchmark_try_to_value,
    benchmark_tree_operations_combined
);
criterion_main!(benches);
