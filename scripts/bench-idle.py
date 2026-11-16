"""空闲基线测量：起 release 二进制，每 10s 采样 VmRSS 与线程数，顺带给出增长斜率。

用法：`python3 scripts/bench-idle.py [秒数]`（默认 240）。需要 `WAYLAND_DISPLAY` 可用，
并在测量前确保没有其它实例占着单实例锁；`XDG_CONFIG_HOME` 指到临时目录以免污染真实配置。
"""
import os, subprocess, time, sys, signal
BIN="/home/pt/Work/RustProjects/dvkit/target/release/docker-panel"
env=dict(os.environ); env["XDG_CONFIG_HOME"]="/tmp/ptrace-cfg3"; env["XDG_RUNTIME_DIR"]="/run/user/1000"
env["WAYLAND_DISPLAY"]="wayland-0"
env["DP_PERF"]="1"
dur=int(sys.argv[1]) if len(sys.argv)>1 else 20
p=subprocess.Popen([BIN], env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
pid=p.pid; t0=time.time()
def rss():
    with open(f"/proc/{pid}/status") as f:
        for line in f:
            if line.startswith("VmRSS"): return int(line.split()[1])//1024
    return 0
def thr():
    return len(os.listdir(f"/proc/{pid}/task"))
samples=[]
while time.time()-t0 < dur:
    time.sleep(10)
    samples.append((round(time.time()-t0), rss(), thr()))
time.sleep(3)
if p.poll() is not None:
    sys.exit("应用启动即退出（检查单实例锁 / Wayland 环境）")
print(" ".join(f"{t}s:{r}M/{n}t" for t,r,n in samples))
print("slope MB/min:", round((samples[-1][1]-samples[0][1]) / ((samples[-1][0]-samples[0][0])/60), 3))
open("/tmp/rss_pid","w").write(str(pid))
p.send_signal(signal.SIGTERM); p.wait(timeout=5)
