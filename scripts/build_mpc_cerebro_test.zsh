#!/usr/bin/env zsh

set -ex

disable -r time

cargo build --release --example cerebro

BIN=./target/release/examples/cerebro

case "$OSTYPE" in 
    darwin*)
        alias measure_time="gtime --format='%e seconds %M kB'"
    ;;
    linux*)
        alias measure_time="time --format='%e seconds %M kB'"
    ;;
esac

function mpc_test {
    parties=$1
    zpath=$2
    RUST_BACKTRACE=1 measure_time $BIN -p $parties $zpath
}

# build mpc arithmetic tests
mpc_test 2 ./examples/Cerebro/mpc/sum_2p.json
mpc_test 2 ./examples/Cerebro/mpc/unroll_reveal.json
mpc_test 2 ./examples/Cerebro/mpc/unroll_test.json
mpc_test 2 ./examples/Cerebro/mpc/mini_cond.json