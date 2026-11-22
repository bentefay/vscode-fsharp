#!/usr/bin/env python3
"""Canonical ZIP metadata/order for locally reproducible VSIX packages."""
import argparse
import datetime
import os
from pathlib import Path
import tempfile
import zipfile


def normalize(filename, epoch):
    timestamp = datetime.datetime.fromtimestamp(epoch, datetime.timezone.utc)
    if not 1980 <= timestamp.year <= 2107:
        raise ValueError('SOURCE_DATE_EPOCH must fit the ZIP date range (1980–2107)')
    date = (timestamp.year, timestamp.month, timestamp.day, timestamp.hour,
            timestamp.minute, timestamp.second // 2 * 2)
    filename = Path(filename)
    with zipfile.ZipFile(filename) as source:
        names = source.namelist()
        if len(names) != len(set(names)):
            raise ValueError('Duplicate VSIX entries are not supported')
        entries = [(item, source.read(item)) for item in sorted(source.infolist(), key=lambda item: item.filename)]
    fd, temporary = tempfile.mkstemp(prefix=filename.name + '.', dir=filename.parent)
    os.close(fd)
    try:
        with zipfile.ZipFile(temporary, 'w', compression=zipfile.ZIP_DEFLATED, compresslevel=9) as target:
            for original, content in entries:
                item = zipfile.ZipInfo(original.filename, date_time=date)
                item.create_system = 3
                item.external_attr = original.external_attr
                item.compress_type = zipfile.ZIP_DEFLATED
                target.writestr(item, content, compresslevel=9)
        os.replace(temporary, filename)
    finally:
        if os.path.exists(temporary):
            os.unlink(temporary)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('vsix', type=Path)
    parser.add_argument('--epoch', type=int, required=True)
    args = parser.parse_args()
    normalize(args.vsix, args.epoch)
