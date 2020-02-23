def plan(n: int) -> None:
    print(f'1..{n}')


def diagnostic(msg: str) -> None:
    print(f'# {msg}')


def ok(description: str) -> None:
    print(f'ok {description}')


def not_ok(description: str) -> None:
    print(f'not ok {description}')