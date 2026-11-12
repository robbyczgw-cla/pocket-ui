"""Park device control files without ever deleting a file owned by another run.

Use a directory on the SAME filesystem (normally /tmp). A failed park leaves
unmoved paths untouched. The caller must remove its own replacements explicitly
before restore; an unexpected replacement is an error, never permission to unlink.
"""
import os
import errno
from pathlib import Path


class SavedFile:
    """Capture the restore target with its bytes, independent of later loop variables."""
    def __init__(self, path):
        self.path = Path(path)
        self.before = self.path.read_bytes()

    def unchanged(self):
        return self.path.read_bytes() == self.before

    def restore(self):
        if not self.unchanged():
            self.path.write_bytes(self.before)


class ParkedFiles:
    def __init__(self, directory):
        self.directory = Path(directory)
        self.moved = []

    def park(self, paths):
        paths = [Path(p) for p in paths if os.path.lexists(p)]
        device = self.directory.stat().st_dev
        # Validate the WHOLE batch before the first mutation.
        for path in paths:
            if path.parent.stat().st_dev != device:
                raise OSError('parking requires the same filesystem: ' + str(path))
            if os.path.lexists(self.directory / path.name):
                raise FileExistsError(self.directory / path.name)
        for path in paths:
            target = self.directory / path.name
            path.rename(target)
            self.moved.append((path, target))

    def restore(self):
        for path, target in reversed(self.moved[:]):
            if os.path.lexists(path):
                raise FileExistsError('refusing to delete an unexpected replacement: ' + str(path))
            target.rename(path)
            self.moved.remove((path, target))


def write_if_reader(path, command):
    """A core that already exited leaves a FIFO with no reader. Never block cleanup."""
    try:
        fd = os.open(path, os.O_WRONLY | os.O_NONBLOCK)
    except OSError as error:
        if error.errno in (errno.ENXIO, errno.ENOENT):
            return False
        raise
    try:
        os.write(fd, command)
    finally:
        os.close(fd)
    return True


def restore_steps(steps):
    """Attempt every independent restore, including resuming the owner after an error."""
    errors = []
    for step in steps:
        try:
            step()
        except Exception as error:
            errors.append(error)
    return errors
