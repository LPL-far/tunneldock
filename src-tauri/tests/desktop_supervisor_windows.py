"""Windows desktop guardian integration checks; no real TunnelDock/Pi is touched.
Run: python src-tauri/tests/desktop_supervisor_windows.py
"""
import ctypes
from ctypes import wintypes
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time
import unittest

GUARD = Path(os.environ.get('TD_GUARDIAN_TEST_EXE', str(Path(__file__).resolve().parents[1] / 'target/release/tunneldock.exe')))

@unittest.skipUnless(sys.platform == 'win32', 'Windows process guardian')
class DesktopGuardianTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix='td-guardian-test-')
        self.root = Path(self.temp.name)
        self.children = []
        self.ps = str(Path(os.environ['SystemRoot']) / 'System32/WindowsPowerShell/v1.0/powershell.exe')
        self.target = self.spawn(['-NoProfile', '-NonInteractive', '-Command', 'Start-Sleep -Seconds 90'])
        k = ctypes.WinDLL('kernel32', use_last_error=True)
        k.OpenProcess.restype = wintypes.HANDLE
        k.GetProcessTimes.argtypes = [wintypes.HANDLE] + [ctypes.POINTER(wintypes.FILETIME)] * 4
        k.CloseHandle.argtypes = [wintypes.HANDLE]
        handle = k.OpenProcess(0x1000, False, self.target.pid)
        self.assertTrue(handle)
        times = [wintypes.FILETIME() for _ in range(4)]
        try:
            self.assertTrue(k.GetProcessTimes(handle, *[ctypes.byref(t) for t in times]))
        finally:
            k.CloseHandle(handle)
        born = ((times[0].dwHighDateTime << 32) | times[0].dwLowDateTime)
        self.lease = dict(version=1, instance_id='isolated-fixture', pid=self.target.pid,
                          started_at=(born-116444736000000000)//10000000,
                          executable=self.ps, cwd=str(self.root),
                          args=['-NoProfile', '-NonInteractive', '-Command', 'exit 1'],
                          mode='standalone', intent='running', ready=True)
        self.save_lease()

    def spawn(self, args):
        p = subprocess.Popen([self.ps, *args], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                             creationflags=0x08000000)
        self.children.append(p)
        return p

    def save_lease(self):
        temp = self.root/'lease.tmp'
        temp.write_text(json.dumps(self.lease), encoding='utf-8')
        os.replace(temp, self.root/'state.json')

    def watch(self):
        p = subprocess.Popen([str(GUARD), '--desktop-guardian', str(self.root)], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, creationflags=0x08000000)
        self.children.append(p)
        return p

    def wait_status(self, desired, timeout=12):
        deadline = time.monotonic()+timeout
        last = None
        while time.monotonic()<deadline:
            try: last = json.loads((self.root/'status.json').read_text(encoding='utf-8-sig'))
            except (OSError, ValueError): pass
            if last and last.get('status') == desired:
                return last
            time.sleep(.1)
        self.fail(f'No {desired}: {last}')

    def tearDown(self):
        for p in reversed(self.children):
            if p.poll() is None:
                p.terminate()
            p.wait(timeout=5)
        self.temp.cleanup()

    def test_intentional_exit_does_not_restart(self):
        watcher = self.watch()
        self.wait_status('watching')
        self.lease['intent'] = 'exit'; self.save_lease()
        self.wait_status('stopped')
        self.assertEqual(watcher.wait(timeout=5), 0)
        self.assertIsNone(self.target.poll())

    def test_update_exit_does_not_restart(self):
        self.lease['intent']='update'; self.save_lease()
        self.watch(); self.wait_status('watching')
        self.target.terminate(); self.target.wait(timeout=5)
        status=self.wait_status('stopped')
        self.assertEqual(status['restart_count'],0)

    def test_recycled_pid_is_not_adopted(self):
        self.lease['started_at']+=1; self.save_lease()
        self.watch()
        self.assertIn('mismatch', self.wait_status('watcher_error')['reason'])
        self.assertIsNone(self.target.poll())

    def test_second_watcher_exits_without_duplicate_loop(self):
        first=self.watch(); self.wait_status('watching')
        second=self.watch()
        self.assertEqual(second.wait(timeout=5),0)
        self.assertIsNone(first.poll())
        self.assertIsNone(self.target.poll())

    def test_repeated_crashes_open_circuit_after_three_attempts(self):
        watcher=self.watch(); self.wait_status('watching')
        self.target.terminate(); self.target.wait(timeout=5)
        status=self.wait_status('circuit_open',timeout=40)
        self.assertEqual(status['restart_count'],3)
        self.assertEqual(watcher.wait(timeout=5),0)

if __name__ == '__main__':
    unittest.main(verbosity=2)
