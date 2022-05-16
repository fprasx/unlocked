#!bin/sh

for i in {20..30}; do
    echo "Running with seed $i"
    MIRIFLAGS="-Zmiri-seed=$i" cargo miri run --target x86_64-unknown-linux-gnu
    if [[ $? -ne 0 ]]; then
        exit 1
    fi
done