# Run miri ten times with different random seeds
for i in (seq 20 30);
    echo "Running with seed $i"
    MIRIFLAGS="-Zmiri-seed=$i" cargo miri run
    if test $status -eq 1
        exit 1
    end
end