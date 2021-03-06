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

    def overlay_read(self, relative: str) -> subprocess.CompletedProcess:
        return subprocess.run(
            ["cat", self.lower / relative], env=self.env, stdout=subprocess.PIPE, stderr=None
        )

    def overlay_write(self, relative: str, contents: bytes) -> subprocess.CompletedProcess:
        return subprocess.run(
            ["tee", self.lower / relative], input=contents, env=self.env, stdout=subprocess.PIPE, stderr=None
        )



def read_all(path: Union[str, Path]) -> bytes:
    with open(path, mode="rb") as file:
        return file.read()


def can_read_lower(env: TestEnv) -> None:
    ret = env.overlay_read("foo.txt")
    assert ret.returncode == 0
    assert ret.stdout == read_all(env.lower / "foo.txt")

    ret = env.overlay_read("bar/bar.txt")
    assert ret.returncode == 0
    assert ret.stdout == read_all(env.lower / "bar" / "bar.txt")

    ret = env.overlay_read("bar/baz.txt")
    assert ret.returncode != 0


def redirect_lower_writes_existing(env: TestEnv) -> None:
    ret = env.overlay_read("foo.txt")
    assert ret.returncode == 0
    assert ret.stdout == read_all(env.lower / "foo.txt")

    ret = env.overlay_write("foo.txt", b"Overwrite")
    assert ret.returncode == 0

    ret = env.overlay_read("foo.txt")
    assert ret.returncode == 0
    assert ret.stdout == b"Overwrite"


def redirect_lower_writes_new(env: TestEnv) -> None:
    ret = env.overlay_write("new_file.txt", b"It is new")
    assert ret.returncode == 0

    ret = env.overlay_write("new_dir/new_file.txt", b"It is new")
    assert ret.returncode != 0

    ret = env.overlay_read("new_file.txt")
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

    ret = env.overlay_write("new_dir/new_file.txt", b"It is new")
    assert ret.returncode == 0

    ret = env.overlay_read("new_dir/new_file.txt")
    assert ret.returncode == 0
    assert ret.stdout == b"It is new"


def redirect_readdir(env: TestEnv) -> None:
    ret = subprocess.run(
        ["ls", env.lower / "bar"], env=env.env, stdout=subprocess.PIPE, stderr=None,
    )
    assert ret.returncode == 0
    assert ret.stdout.splitlines() == [b"bar.txt"]

    ret = env.overlay_write("bar/baz.txt", b"It is new")
    assert ret.returncode == 0

    ret = subprocess.run(
        ["ls", env.lower / "bar"], env=env.env, stdout=subprocess.PIPE, stderr=None,
    )
    assert ret.returncode == 0
    assert sorted(ret.stdout.splitlines()) == [b"bar.txt", b"baz.txt"]

    ret = env.overlay_write("bar/bar.txt", b"It is new")
    assert ret.returncode == 0

    ret = subprocess.run(
        ["ls", env.lower / "bar"], env=env.env, stdout=subprocess.PIPE, stderr=None,
    )
    assert ret.returncode == 0
    tap.diagnostic(str(ret.stdout.splitlines()))
    assert sorted(ret.stdout.splitlines()) == [b"bar.txt", b"baz.txt"]


def redirect_stat(env: TestEnv) -> None:
    ret = env.overlay_write("new_file.txt", b"It is new")
    assert ret.returncode == 0

    ret = subprocess.run(
        ["stat", env.lower / "foo.txt"],
        env=env.env,
        stdout=subprocess.PIPE,
        stderr=None,
    )
    assert ret.returncode == 0

    ret = subprocess.run(
        ["stat", env.lower / "new_file.txt"],
        env=env.env,
        stdout=subprocess.PIPE,
        stderr=None,
    )
    assert ret.returncode == 0


def redirect_unlink(env: TestEnv) -> None:
    ret = env.overlay_write("new_file.txt", b"It is new")
    assert ret.returncode == 0

    ret = env.overlay_write("foo.txt", b"It is new")
    assert ret.returncode == 0

    ret = subprocess.run(
        ["unlink", env.lower / "foo.txt"],
        input=b"n\n",
        env=env.env,
        stdout=subprocess.PIPE,
        stderr=None,
    )
    assert ret.returncode == 0

    ret = subprocess.run(
        ["unlink", env.lower / "new_file.txt"],
        input=b"n\n",
        env=env.env,
        stdout=subprocess.PIPE,
        stderr=None,
    )
    assert ret.returncode == 0

    # Deletion should fail if it hits the underlying directory
    ret = subprocess.run(
        ["unlink", env.lower / "foo.txt"],
        input=b"n\n",
        env=env.env,
        stdout=subprocess.PIPE,
        stderr=None,
    )
    assert ret.returncode != 0


def redirect_rmdir(env: TestEnv) -> None:
    ret = subprocess.run(
        ["rmdir", env.lower / "new_dir"],
        input=b"n\n",
        env=env.env,
        stdout=subprocess.PIPE,
        stderr=None,
    )
    assert ret.returncode != 0

    ret = subprocess.run(
        ["mkdir", env.lower / "new_dir"],
        input=b"n\n",
        env=env.env,
        stdout=subprocess.PIPE,
        stderr=None,
    )
    assert ret.returncode == 0

    ret = subprocess.run(
        ["rmdir", env.lower / "new_dir"],
        input=b"n\n",
        env=env.env,
        stdout=subprocess.PIPE,
        stderr=None,
    )
    assert ret.returncode == 0


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
    tests = [
        can_read_lower,
        redirect_lower_writes_existing,
        redirect_lower_writes_new,
        redirect_mkdir,
        redirect_readdir,
        redirect_stat,
        redirect_unlink,
        redirect_rmdir,
    ]

    tap.plan(len(tests))
    for test in tests:
        run_test(test)


if __name__ == "__main__":
    run()
