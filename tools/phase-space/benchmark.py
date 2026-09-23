#!/usr/bin/env python3
"""Execute the fixed 12-case, three-repeat protocol once. Never replace runs."""
import argparse, datetime, hashlib, itertools, json, os, pathlib, random, subprocess, sys

def git(root,*args):
    return subprocess.check_output(['git',*args],cwd=root,text=True).strip()

def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('--binary',type=pathlib.Path,required=True)
    p.add_argument('--output',type=pathlib.Path,required=True)
    a=p.parse_args();root=pathlib.Path(__file__).resolve().parents[2]
    binary=a.binary.resolve();output=a.output.resolve()
    if output.is_relative_to(root):p.error('recorded output must be outside checkout')
    if git(root,'status','--porcelain'):p.error('recorded benchmark requires a clean checkout')
    output.mkdir(parents=True,exist_ok=False)
    cases=list(itertools.product(['rhythmic','noise','impulse'],['channels','delay'],['trace','full']))
    schedule=[]
    for repeat in range(1,4):
        round_cases=cases.copy();random.Random(20260923+repeat).shuffle(round_cases)
        schedule.extend([dict(fixture=f,mapping=m,layers=l,repeat=repeat) for f,m,l in round_cases])
    manifest=dict(schema='neuralcompose.phase-space.benchmark.v1',commit=git(root,'rev-parse','HEAD'),
        executable_sha256=hashlib.sha256(binary.read_bytes()).hexdigest(),
        started_utc=datetime.datetime.now(datetime.timezone.utc).isoformat(),
        machine=os.uname()._asdict() if hasattr(os.uname(),'_asdict') else list(os.uname()),
        protocol='12 cases x 3 runs; 5s warmup + 60s measurement; no replacement or extension runs',
        verdict='3/3 both targets: accepted; 0/3: targets not met; 1/3 or 2/3: unresolved',
        targets=dict(frame_p95_ms=20,sample_to_submit_p95_ms=100),schedule=schedule,
        graphics_environment={key:os.environ.get(key) for key in
            ['DISPLAY','WAYLAND_DISPLAY','WINIT_UNIX_BACKEND','WGPU_BACKEND','VK_DRIVER_FILES']})
    # Desktop refresh is evidence from the compositor, not an assumed 60 Hz.
    for command,label in [(['xrandr','--current'],'display'),(['vulkaninfo','--summary'],'vulkan'),
                          (['lscpu'],'cpu'),(['powerprofilesctl','get'],'power-profile')]:
        try:
            r=subprocess.run(command,capture_output=True,text=True)
            (output/(label+'.txt')).write_text(r.stdout+r.stderr)
        except FileNotFoundError:
            (output/(label+'.txt')).write_text('Unavailable: command not installed\n')
    (output/'manifest.json').write_text(json.dumps(manifest,indent=2)+'\n')
    failed=[]
    for index,case in enumerate(schedule,1):
        name=f"{case['fixture']}-{case['mapping']}-{case['layers']}-r{case['repeat']}"
        if git(root,'status','--porcelain') or git(root,'rev-parse','HEAD')!=manifest['commit']:
            raise SystemExit('checkout changed during benchmark; remaining cases are not run')
        print(f"[{index}/36] {name}",flush=True)
        argv=[str(binary),'--demo','--fixture',case['fixture'],'--mapping',case['mapping'],
              '--repeat',str(case['repeat']),'--warmup','5','--seconds','60','--recorded',
              '--metrics-out',str(output/(name+'.json'))]
        if case['layers']=='trace':argv.append('--trace-only')
        try:
            r=subprocess.run(argv,cwd=root,capture_output=True,text=True,timeout=90)
            (output/(name+'.stdout.txt')).write_text(r.stdout+r.stderr)
            result_path=output/(name+'.json')
            if r.returncode:failed.append(dict(case=name,reason=f'exit {r.returncode}'))
            elif not result_path.exists():failed.append(dict(case=name,reason='missing measurement file'))
            elif not json.loads(result_path.read_text()).get('quotable'):
                failed.append(dict(case=name,reason='incomplete or non-quotable measurement'))
            print(r.stdout.strip() or f'exit {r.returncode}',flush=True)
        except subprocess.TimeoutExpired as e:
            failed.append(dict(case=name,reason='timeout; not replaced'))
            (output/(name+'.stdout.txt')).write_text(str(e))
        # Continuing the registered schedule is not replacing a failed run.
        (output/'failures.json').write_text(json.dumps(failed,indent=2)+'\n')
    manifest['ended_utc']=datetime.datetime.now(datetime.timezone.utc).isoformat()
    manifest['failures']=failed
    (output/'manifest.json').write_text(json.dumps(manifest,indent=2)+'\n')
    print(f'Finished registered schedule; {len(failed)} failed runs retained.',flush=True)
if __name__=='__main__':main()
