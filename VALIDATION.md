# Validation

In order to prove the core is 100% compatible with the reference python
implementation, we created some scripts that will record calls from a real ROS
system and then we can replay them back against our implementation and the
reference.

To test this, you can run this core which will generate debug output logs using
this command

```bash
RUST_LOG=debug cargo run --release --bin ros-core-rs |& tee output.log
# Just to ensure we have a run_id set, run this in another terminal. And uuid will do.
rosparam set /run_id 2ffbf8a4-5cb6-11f0-b764-13f1aaf73515
```

After this, run your ROS code and then we will log all of the calls made to us
into `output.log`. We then have a conversion script which translates those logs
into json for easier replaying.

```bash
python3 scripts/convert_log_to_json.py output.log output.jsonl
```

This will create an `output.jsonl` file with all the calls, and the responses we
gave to them. Finally, we can compare those to running the real ROS master.

```bash
rosmaster
# In another terminal
cargo run --bin replay_log -- output.jsonl
```

This will generate an output which shows any errors, and the performance of it.
If you want to compare performance, you can run the `ros-core-rs` command again
and replay against it!

```bash
cargo run --release --bin ros-core-rs
# In another terminal
cargo run --bin replay_log -- output.jsonl
```
