"""Verify signed timedelta reference values used by the NoneBot config audit.

Pydantic v1 datetime_parse.parse_duration passes the signed day/clock components
individually to datetime.timedelta. A leading minus on a day-clock string must
therefore never be silently reinterpreted as a minus on the entire duration.
"""

from datetime import timedelta


def main() -> None:
    signed_components = timedelta(
        days=-1, hours=2, minutes=3, seconds=4, microseconds=5
    )
    assert signed_components == -timedelta(seconds=79015, microseconds=999995)
    assert signed_components != -timedelta(
        days=1, hours=2, minutes=3, seconds=4, microseconds=5
    )
    assert timedelta(hours=-0, minutes=2) == timedelta(seconds=120)
    assert -timedelta(minutes=2) == timedelta(seconds=-120)
    print("stdlib signed day-clock reference passed:", signed_components)


if __name__ == "__main__":
    main()
