#!/bin/zsh
# Run this ON THE M3:   ~/bench/esp32sim/x2/start.sh          (safe to run any time; does nothing if already running)
#   ~/bench/esp32sim/x2/start.sh status     table of finished runs + what is running now
#   ~/bench/esp32sim/x2/start.sh stop       finish the current job, then stop
# The runner is detached (nohup + caffeinate): closing the terminal, ssh or the laptop session does not stop it.
# Keep Chrome/Safari closed while it runs (it waits, it never kills anything). Power + lid open, or it sleeps.
X=$HOME/bench/esp32sim/x2; cd $X || exit 1
case "${1:-start}" in
  status)
    for f in runs/*/summary.json(N); do node -e 'const d=require(process.argv[1]);console.log(process.argv[2].padEnd(28),String(d.wallReductionPercent.toFixed(2)).padStart(7)+"%  pairs:",d.pairsWallReductionPercent.map(x=>x.toFixed(2)).join(", "))' $X/$f ${f:h:t}; done
    for f in runs/*.failed(N); do echo "FAILED ${f:t:r} (see logs/${f:t:r}.log)"; done
    echo "--- waiting:"; while read -r n w p rest; do [[ -z "$n" || "$n" == \#* ]] && continue; [[ -e runs/$n/summary.json || -e runs/$n.failed ]] || echo "  $n"; done < jobs.txt
    echo "--- log:"; tail -3 runner.log; pgrep -fl 'runner.sh' >/dev/null && echo "runner: RUNNING" || echo "runner: NOT running";;
  stop) touch STOP; echo "will stop after the current job (rm $X/STOP to cancel)";;
  start)
    if pgrep -f 'x2/runner.sh|\./runner.sh' >/dev/null; then echo "runner already running"; tail -2 runner.log; exit 0; fi
    rm -f STOP; [ -d .runner.lock ] && { echo "removing stale lock"; rmdir .runner.lock; }
    nohup $HOME/bench/esp32sim/env.sh caffeinate -dims ./runner.sh >> runner.log 2>&1 < /dev/null &!
    sleep 2; echo "started"; tail -2 runner.log;;
esac
