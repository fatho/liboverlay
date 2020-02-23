#!/usr/bin/env python3.7

import os
import sys
import subprocess
import tempfile
import traceback
from pathlib import Path
from typing import Callable, Mapping, NamedTuple, Union

import tap

SCRIPT_DIR = os.path.dirname(os.path.realpath(__file__))


class TestEnv(NamedTuple):
    lower: Path
    upper: Path
    env: Mapping[str, str]


def read_all(path: Union[str, Path]) -> bytes:
    with open(path, mode="rb") as file:
        return file.read()


def exec_bash(script: str) -> bytes:
    return subprocess.check_output(["bash", "-c", script])


def can_read_lower(env: TestEnv) -> None:
    ret = subprocess.run(
        ["cat", env.lower / "foo.txt"], env=env.env, stdout=subprocess.PIPE, stderr=None
    )
    assert ret.returncode == 0
    assert ret.stdout == read_all(env.lower / "foo.txt")

    ret = subprocess.run(
        ["cat", env.lower / "bar" / "bar.txt"],
        env=env.env,
        stdout=subprocess.PIPE,
        stderr=None,
    )
    assert ret.returncode == 0
    assert ret.stdout == read_all(env.lower / "bar" / "bar.txt")

    ret = subprocess.run(
        ["cat", env.lower / "bar" / "baz.txt"],
        env=env.env,
        stdout=subprocess.PIPE,
        stderr=None,
    )
    assert ret.returncode != 0


def redirect_lower_writes_existing(env: TestEnv) -> None:
    ret = subprocess.run(
        ["cat", env.lower / "foo.txt"], env=env.env, stdout=subprocess.PIPE, stderr=None
    )
    assert ret.returncode == 0
    assert ret.stdout == read_all(env.lower / "foo.txt")

    ret = subprocess.run(
        ["tee", env.lower / "foo.txt"],
        input=b"Overwrite",
        env=env.env,
        stdout=subprocess.PIPE,
        stderr=None,
    )
    assert ret.returncode == 0

    ret = subprocess.run(
        ["cat", env.lower / "foo.txt"], env=env.env, stdout=subprocess.PIPE, stderr=None
    )
    assert ret.returncode == 0
    assert ret.stdout == b"Overwrite"


def redirect_lower_writes_new(env: TestEnv) -> None:
    ret = subprocess.run(
        ["tee", env.lower / "new_file.txt"],
        input=b"It is new",
        env=env.env,
        stdout=subprocess.PIPE,
        stderr=None,
    )
    assert ret.returncode == 0

    ret = subprocess.run(
        ["tee", env.lower / 'new_dir' / "new_file.txt"],
        input=b"It is new",
        env=env.env,
        stdout=subprocess.PIPE,
        stderr=None,
    )
    assert ret.returncode != 0

    ret = subprocess.run(
        ["cat", env.lower / "new_file.txt"],
        env=env.env,
        stdout=subprocess.PIPE,
        stderr=None,
    )
    assert ret.returncode == 0
    assert ret.stdout == b"It is new"


def redirect_mkdir(env: TestEnv) -> None:
    ret = subprocess.run(
        ["mkdir", env.lower / "new_dir"],
        env=env.env,
        stdout=subprocess.PIPE,
        stderr=None,
    )
    assert ret.returncode == 0

    ret = subprocess.run(
        ["tee", env.lower / 'new_dir' / "new_file.txt"],
        input=b"It is new",
        env=env.env,
        stdout=subprocess.PIPE,
        stderr=None,
    )
    assert ret.returncode == 0

    ret = subprocess.run(
        ["cat", env.lower / "new_dir" / "new_file.txt"],
        env=env.env,
        stdout=subprocess.PIPE,
        stderr=None,
    )
    assert ret.returncode == 0
    assert ret.stdout == b"It is new"


def run_test(test: Callable[[TestEnv], None]) -> None:
    with tempfile.TemporaryDirectory() as upper_dir:
        env = os.environ.copy()
        lower_dir = f"{SCRIPT_DIR}/lower"
        env["LIBOVERLAY_LOWER_DIR"] = lower_dir
        env["LIBOVERLAY_UPPER_DIR"] = upper_dir
        env["LD_PRELOAD"] = os.path.realpath(
            f"{SCRIPT_DIR}/../target/debug/liboverlay.so"
        )

        try:
            test(TestEnv(upper=Path(upper_dir), lower=Path(lower_dir), env=env))
        except:
            tap.not_ok(f"{test.__name__}")
            for line in traceback.format_exc().splitlines():
                tap.diagnostic(line)
        else:
            tap.ok(f"{test.__name__}")


def run():
    tests = [can_read_lower, redirect_lower_writes_existing, redirect_lower_writes_new, redirect_mkdir]

    tap.plan(len(tests))
    for test in tests:
        run_test(test)


if __name__ == "__main__":
    run()
