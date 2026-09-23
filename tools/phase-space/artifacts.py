#!/usr/bin/env python3
"""Adapt retained Office templates without replacing their layout or styles."""
import argparse, collections, copy, hashlib, itertools, json, math, pathlib, statistics, zipfile
from xml.etree import ElementTree as E

ROOT=pathlib.Path('/home/aurascoper/.codex/plugins/cache/openai-curated-remote/openai-templates/0.1.1/skills')
S='http://schemas.openxmlformats.org/spreadsheetml/2006/main'; W='http://schemas.openxmlformats.org/wordprocessingml/2006/main'
R='http://schemas.openxmlformats.org/officeDocument/2006/relationships'; C='http://schemas.openxmlformats.org/drawingml/2006/chart'
CASES=['-'.join(c) for c in itertools.product(['rhythmic','noise','impulse'],['channels','delay'],['trace','full'])]

def tag(ns,name):return '{'+ns+'}'+name

def text(node,ns=W):return ''.join(t.text or '' for t in node.iter(tag(ns,'t')))

def paragraph(p,value):
    ts=list(p.iter(tag(W,'t')))
    if ts:
        ts[0].text=str(value)
        for t in ts[1:]:t.text=''
    else:
        r=E.SubElement(p,tag(W,'r'));E.SubElement(r,tag(W,'t')).text=str(value)

def verdict(rows):
    if len(rows)!=3 or {r['repeat'] for r in rows}!={1,2,3} or any(not r['quotable'] or not r['clean_tree'] for r in rows):return 'unresolved'
    if len({(r['commit'],r['executable_sha256'],r['case']) for r in rows})!=1:return 'unresolved'
    hits=sum(r['frame_p95_ms'] is not None and r['age_p95_ms'] is not None and r['frame_p95_ms']<=20 and r['age_p95_ms']<=100 for r in rows)
    return {0:'targets not met',1:'unresolved',2:'unresolved',3:'accepted'}[hits]

def spread(rows,key):
    values=[r[key] for r in rows if r.get(key) is not None]
    return None if not values else (min(values),statistics.median(values),max(values))

def fmt(value):return 'Not measured' if value is None else f'{value:.2f}'

def zip_write(source,out,parts):
    with zipfile.ZipFile(source) as zin,zipfile.ZipFile(out,'w',zipfile.ZIP_DEFLATED) as zout:
        names=set()
        for member in zin.infolist():
            zout.writestr(member,parts.get(member.filename,zin.read(member.filename)));names.add(member.filename)
        for name,data in parts.items():
            if name not in names:zout.writestr(name,data)

def xml_bytes(root):return E.tostring(root,encoding='utf-8',xml_declaration=True)

def workbook(source,out,groups,runs,manifest):
    E.register_namespace('',S);E.register_namespace('r',R)
    with zipfile.ZipFile(source) as z:parts={n:z.read(n) for n in z.namelist()}
    sheets=[E.fromstring(parts[f'xl/worksheets/sheet{i}.xml']) for i in range(1,4)]
    strings=[]
    if 'xl/sharedStrings.xml' in parts:
        strings=[text(n,S) for n in E.fromstring(parts['xl/sharedStrings.xml'])]
    def value(cell):
        v=cell.find(tag(S,'v'))
        if cell.get('t')=='s':return strings[int(v.text)]
        return text(cell,S) if cell.get('t')=='inlineStr' else (v.text if v is not None else '')
    def get(root,ref):
        for c in root.iter(tag(S,'c')):
            if c.get('r')==ref:return c
        sd=root.find(tag(S,'sheetData'));number=''.join(x for x in ref if x.isdigit())
        row=next((r for r in sd if r.get('r')==number),None)
        if row is None:row=E.SubElement(sd,tag(S,'row'),r=number)
        return E.SubElement(row,tag(S,'c'),r=ref)
    styles=E.fromstring(parts['xl/styles.xml']);formats=styles.find(tag(S,'numFmts'))
    if formats is None:formats=E.Element(tag(S,'numFmts'),count='0');styles.insert(0,formats)
    xfs=styles.find(tag(S,'cellXfs'));style_cache={}
    def setcell(root,ref,v=None,formula=None,number_format=None):
        c=get(root,ref)
        for child in list(c):c.remove(child)
        c.attrib.pop('t',None)
        if number_format:
            key=(c.get('s','0'),number_format)
            if key not in style_cache:
                numid=200+len(formats);E.SubElement(formats,tag(S,'numFmt'),numFmtId=str(numid),formatCode=number_format)
                xf=copy.deepcopy(xfs[int(key[0])]);xf.set('numFmtId',str(numid));xf.set('applyNumberFormat','1');xfs.append(xf);style_cache[key]=len(xfs)-1
            c.set('s',str(style_cache[key]))
        if formula is not None:
            E.SubElement(c,tag(S,'f')).text=formula
            if isinstance(v,str):c.set('t','str')
            if v is not None:E.SubElement(c,tag(S,'v')).text=str(v)
        elif isinstance(v,str):
            c.set('t','inlineStr');E.SubElement(E.SubElement(c,tag(S,'is')),tag(S,'t')).text=v
        elif v is not None:E.SubElement(c,tag(S,'v')).text=str(v)
    # Preserve template cells/styles/merges, but remove every example value and
    # shared formula before supplying engineering content. Stale caches are data.
    dash,data,helpers=sheets
    for root in sheets:
        for c in list(root.iter(tag(S,'c'))):setcell(root,c.get('r'),None)
    for ref,label in {'B3':'Engineering measurements','J2':'SELECTED CASE','N2':'RECORDED RUNS',
                      'B5':'FRAME P95 · MS','F5':'AGE P95 · MS','J5':'LIVE DISPLAY','N5':'WITHIN 20 MS',
                      'B10':'MEDIAN FPS','F10':'ACCEPTED SAMPLES','J10':'CLIPPED FRACTION','N10':'SKIPPED NOTIFICATIONS',
                      'B15':'FRAME P95 — WORST RUN VS TARGET','J15':'AGE P95 — WORST RUN VS TARGET',
                      'B44':'Metric','C44':'Direction','D44':'Observed','E44':'Reference',
                      'F44':'Difference','G44':'Difference %','H44':'Reference status'}.items():setcell(dash,ref,label)
    setcell(dash,'J3',CASES[0],formula="'Data & Targets'!$C$9",number_format='General')
    setcell(data,'B3','Fixed engineering comparison');setcell(data,'B5','METHOD AND PROVENANCE')
    setcell(data,'B11','Source')
    setcell(dash,'B2','Phase-space Engineering Dashboard');setcell(data,'B2','Phase-space Engineering Dashboard')
    setcell(data,'C9',CASES[0]);setcell(data,'B9','Selected Case');setcell(data,'B10','Recorded Runs');setcell(data,'C10',len(runs),number_format='0')
    setcell(dash,'N3',len(runs),number_format='0');setcell(data,'B13','CASE SUMMARY — THREE FIXED REPEATS; NO POOLED P95')
    setcell(data,'B6','Dashboard p95 values are the worst of three run-level p95s. Runs holds every measured repetition. No extra or replacement runs.')
    setcell(data,'B7','Select a case in C9. A case needs three quotable runs: 3/3 accepted; 0/3 targets not met; mixed or incomplete unresolved.')
    source_note=f"Build {manifest.get('commit','not measured')[:12]}; {len(runs)}/36 runs. Dirty runs are non-quotable. See Runs and report for provenance."
    setcell(data,'C11',source_note);setcell(dash,'B4',source_note)
    cols=['Case','Frame p95 ms','Age p95 ms','Live fraction','Within 20ms','Median FPS','Accepted samples','Clipped fraction','Frame target','Age target','Live reference','Budget reference','FPS reference','Samples reference','Clip reference']
    for col,label in zip('BCDEFGHIJKLMNOP',cols):setcell(data,col+'15',label)
    summaries={}
    for row,case in enumerate(CASES,16):
        rs=groups.get(case,[])
        def val(key,fn):
            xs=[r[key] for r in rs if r.get(key) is not None];return fn(xs) if xs else None
        vals=[val('frame_p95_ms',max),val('age_p95_ms',max),val('live_fraction',min),val('within_budget_fraction',min),val('fps',statistics.median),val('accepted_samples',sum),val('clipped_fraction',max)]
        summaries[case]=vals;setcell(data,'B'+str(row),case,number_format='General')
        for col,v in zip('CDEFGHI',vals):setcell(data,col+str(row),v,number_format='0.0%' if col in 'EFI' else '0.00')
        for col,v in zip('JKLMNOP',[20,100,1,0.95,60,None,None]):setcell(data,col+str(row),v,number_format='0.0%' if col in 'LMP' else '0.00')
    for column in data.find(tag(S,'cols')):
        if column.get('min')=='2' and column.get('max')=='2':column.set('width','32')
    metric_names=['Frame p95 ms','Age p95 ms','Live fraction','Within 20ms','Median FPS','Accepted samples','Clipped fraction','Skipped notifications']
    selected=CASES[0];selected_values=summaries[selected]+[sum(r.get('skipped_notifications',0) for r in groups.get(selected,[])) if groups.get(selected) else None]
    for row,(label,actual) in enumerate(zip(metric_names,selected_values),2):
        setcell(helpers,'A'+str(row),label)
        idx="MATCH('Data & Targets'!$C$9,'Data & Targets'!$B$16:$B$27,0)"
        if row<=8:
            col='CDEFGHI'[row-2];target='JKLMNOP'[row-2]
            f=f"IF(COUNT(INDEX('Data & Targets'!${col}$16:${col}$27,{idx}))=0,\"\",INDEX('Data & Targets'!${col}$16:${col}$27,{idx}))"
            setcell(helpers,'B'+str(row),actual,formula=f)
            setcell(helpers,'C'+str(row),[20,100,1,0.95,60,None,None][row-2],formula=f"IF(COUNT(INDEX('Data & Targets'!${target}$16:${target}$27,{idx}))=0,\"\",INDEX('Data & Targets'!${target}$16:${target}$27,{idx}))")
        else:setcell(helpers,'B9',actual,formula="IF(COUNTIF(Runs!$A$2:$A$37,'Data & Targets'!$C$9)=0,\"\",SUMIF(Runs!$A$2:$A$37,'Data & Targets'!$C$9,Runs!$O$2:$O$37))")
        setcell(helpers,'D'+str(row),None) # Cross-case 'prior' is not a valid comparison.
        setcell(helpers,'E'+str(row),'Lower' if row in (2,3,8,9) else 'Higher')
        setcell(helpers,'F'+str(row),'Observed',formula=f'IF(B{row}="","Not measured",IF(C{row}="","Observed",IF(IF(E{row}="Lower",B{row}<=C{row},B{row}>=C{row}),"Within reference","Outside reference")))')
    for ref,v,style in zip(['B6','F6','J6','N6','B11','F11','J11','N11'],selected_values,['0.00','0.00','0.0%','0.0%','0.00','0','0.0%','0']):
        row=['B6','F6','J6','N6','B11','F11','J11','N11'].index(ref)+2
        setcell(dash,ref,v if v is not None else 'Not measured',formula=f"IF('_Chart Helpers'!B{row}=\"\",\"Not measured\",'_Chart Helpers'!B{row})",number_format=style)
    # Rebuild each variance cell explicitly; template shared-formula caches must
    # never survive a change of metric or turn an unavailable input into zero.
    for row in range(45,53):
        hrow=row-43
        for col,hcol in [('B','A'),('C','E'),('D','B'),('E','C'),('H','F')]:
            ref=f"'_Chart Helpers'!{hcol}{hrow}"
            setcell(dash,f'{col}{row}',formula=f'IF({ref}="","",{ref})',number_format='General' if col in 'BCH' else ('0.0%' if hrow in (4,5,8) else '0.00'))
        setcell(dash,f'F{row}',formula=f'IF(OR(D{row}="",E{row}=""),"",D{row}-E{row})',number_format='0.00')
        setcell(dash,f'G{row}',formula=f'IF(OR(D{row}="",E{row}=""),"",IFERROR(D{row}/E{row}-1,""))',number_format='0.0%')
    for row in (8,13):
        for col in 'BFJN':setcell(dash,f'{col}{row}','3 repeats')
    for ref,value in [('D8',20),('H8',100),('L8',1),('P8',0.95),('D13',60)]:setcell(dash,ref,value,number_format='0.0%' if ref in ('L8','P8') else '0.00')
    for ref in ['E8','I8']:setcell(dash,ref,'limit')
    for ref in ['M8','Q8','E13']:setcell(dash,ref,'reference')
    for row,case in enumerate(CASES,2):
        drow=row+14;setcell(helpers,'H'+str(row),case,formula=f"'Data & Targets'!B{drow}");setcell(helpers,'L'+str(row),case,formula=f"'Data & Targets'!B{drow}")
        for col,sourcecol in [('I','C'),('J','J'),('M','D'),('N','K')]:setcell(helpers,col+str(row),None,formula=f"IF(COUNT('Data & Targets'!{sourcecol}{drow})=0,NA(),'Data & Targets'!{sourcecol}{drow})")
    counts=[sum(1 for r in runs for x in r['frame_intervals_ms'] if lo<=x<hi) for lo,hi in [(0,10),(10,20),(20,30),(30,50),(50,math.inf)]]
    for row,(label,v) in enumerate(zip(['<10 ms','10–20 ms','20–30 ms','30–50 ms','≥50 ms'],counts),2):setcell(helpers,'P'+str(row),label);setcell(helpers,'Q'+str(row),v if runs else None)
    statuses=collections.Counter(verdict(groups.get(c,[])) for c in CASES)
    for row,label in enumerate(['accepted','targets not met','unresolved','not measured'],2):setcell(helpers,'S'+str(row),label);setcell(helpers,'T'+str(row),statuses[label] if runs else None)
    # Replace the template's business-specific funnel/mix input blocks and notes.
    for c in list(data.iter(tag(S,'c'))):
        if int(''.join(ch for ch in c.get('r') if ch.isdigit()))>=29:setcell(data,c.get('r'),None)
    setcell(data,'B29','RECORDED PROTOCOL');setcell(data,'B30','3 fixed repeats per case');setcell(data,'B31','Mixed results remain unresolved');setcell(data,'B32','No extra or replacement runs');setcell(data,'B33','Only clean release builds are quotable')
    setcell(data,'G29','EVIDENCE BOUNDARY');setcell(data,'G30','No cognition or efficacy claim');setcell(data,'G31','Frame timing is submission cadence');setcell(data,'G32','Age begins at viewer receipt');setcell(data,'G33','Source templates remain unchanged')
    setcell(dash,'B29','FRAME INTERVAL DISTRIBUTION — ALL RECORDED FRAMES');setcell(dash,'J29','CASE VERDICTS — FIXED THREE-RUN RULE')
    setcell(dash,'B43','MEASURED ENGINEERING RESULTS — NO COGNITIVE INFERENCE')
    setcell(dash,'J45','Three p95 values per case; min/median/max are in the report. No pooled percentile. Historical logs supply text and scores only.')
    setcell(dash,'J50','Targets apply only to this recorded hardware and build. Unresolved cases remain unresolved; follow-up requires a new protocol.')
    # Existing selection validation keeps its source range, now containing case IDs.
    run_sheet=E.Element(tag(S,'worksheet'));E.SubElement(run_sheet,tag(S,'dimension'),ref='A1:Q37');E.SubElement(run_sheet,tag(S,'sheetViews'))
    sv=E.SubElement(run_sheet.find(tag(S,'sheetViews')),tag(S,'sheetView'),workbookViewId='0');E.SubElement(sv,tag(S,'pane'),ySplit='1',topLeftCell='A2',activePane='bottomLeft',state='frozen')
    columns=E.SubElement(run_sheet,tag(S,'cols'));E.SubElement(columns,tag(S,'col'),min='1',max='1',width='32',customWidth='1');E.SubElement(columns,tag(S,'col'),min='2',max='17',width='20',customWidth='1');E.SubElement(run_sheet,tag(S,'sheetData'))
    headers=['Case','Repeat','Quotable','Frame p95 ms','Age p95 ms','FPS','Duration s','Rendered frames','Accepted samples','Live fraction','Within budget','Clipped fraction','Commit','Executable SHA256','Skipped notifications','GPU','Framebuffer']
    for col,label in zip('ABCDEFGHIJKLMNOPQ',headers):setcell(run_sheet,col+'1',label)
    for row,r in enumerate(runs,2):
        values=[r['case'],r['repeat'],str(r['quotable']),r['frame_p95_ms'],r['age_p95_ms'],r['fps'],r['duration_s'],r['rendered_frames'],r['accepted_samples'],r['live_fraction'],r['within_budget_fraction'],r['clipped_fraction'],r['commit'],r['executable_sha256'],r['skipped_notifications'],r['gpu'],str(r.get('framebuffer'))]
        for col,v in zip('ABCDEFGHIJKLMNOPQ',values):setcell(run_sheet,col+str(row),v)
    book=E.fromstring(parts['xl/workbook.xml']);E.SubElement(book.find(tag(S,'sheets')),tag(S,'sheet'),name='Runs',sheetId='4',attrib={tag(R,'id'):'rIdRuns'})
    for dn in book.findall('.//'+tag(S,'definedName')):
        if dn.text and '2025' in dn.text:dn.text=dn.text.replace('2025','2026')
    calc=book.find(tag(S,'calcPr'))
    if calc is not None:
        calc.set('fullCalcOnLoad','1');calc.set('forceFullCalc','1');calc.set('calcMode','auto')
    relns='http://schemas.openxmlformats.org/package/2006/relationships';rels=E.fromstring(parts['xl/_rels/workbook.xml.rels']);E.SubElement(rels,tag(relns,'Relationship'),Id='rIdRuns',Type=R+'/worksheet',Target='worksheets/sheet4.xml')
    ct='http://schemas.openxmlformats.org/package/2006/content-types';types=E.fromstring(parts['[Content_Types].xml']);E.SubElement(types,tag(ct,'Override'),PartName='/xl/worksheets/sheet4.xml',ContentType='application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml')
    formats.set('count',str(len(formats)));xfs.set('count',str(len(xfs)))
    for i,root in enumerate(sheets+[run_sheet],1):parts[f'xl/worksheets/sheet{i}.xml']=xml_bytes(root)
    parts['xl/styles.xml']=xml_bytes(styles);parts['xl/workbook.xml']=xml_bytes(book);parts['xl/_rels/workbook.xml.rels']=xml_bytes(rels);parts['[Content_Types].xml']=xml_bytes(types)
    # Remove stale chart caches so renderers read the adapted helper cells.
    for name in list(parts):
        if name.startswith('xl/charts/chart') and name.endswith('.xml'):
            chart=E.fromstring(parts[name])
            for axis in list(chart.iter(tag(C,'catAx')))+list(chart.iter(tag(C,'valAx'))):
                for title in axis.findall(tag(C,'title')):axis.remove(title)
            if not runs:
                for parent in chart.iter():
                    for series in list(parent.findall(tag(C,'ser'))):
                        if name.endswith(('chart1.xml','chart2.xml')) and series.find(tag(C,'idx')).get('val')!='0':continue
                        parent.remove(series)
            chart_labels={'Active users vs target':'Frame p95 by case (ms)','MRR vs target':'Age p95 by case (ms)',
                'Visitor-to-paid funnel':'Frame intervals','Selected-period MRR mix':'Case verdicts',
                'Active Users':'Frame p95','MRR':'Age p95','Users':'Frames','Axis Title':''}
            for node in chart.iter():
                if node.text in chart_labels:node.text=chart_labels[node.text]
                if node.tag==tag(C,'numFmt'):node.set('formatCode','0.0');node.set('sourceLinked','0')
            for parent in chart.iter():
                for node in list(parent):
                    if node.tag in (tag(C,'numCache'),tag(C,'strCache')):parent.remove(node)
            parts[name]=xml_bytes(chart)
    zip_write(source,out,parts)

def document(source,out,groups,runs,manifest):
    E.register_namespace('w',W);E.register_namespace('r',R)
    with zipfile.ZipFile(source) as z:root=E.fromstring(z.read('word/document.xml'))
    date=manifest.get('ended_utc',manifest.get('started_utc','Not measured'))
    commit=manifest.get('commit','Not measured');a=[r for r in runs if r['case'].endswith('-trace')];b=[r for r in runs if r['case'].endswith('-full')]
    def stat(rs,key):
        values=[r[key] for r in rs if r.get(key) is not None];return max(values) if values else None
    def delta(x,y):return 'Not measured' if x is None or y is None else f'{y-x:.2f}'
    counts=collections.Counter(verdict(groups.get(c,[])) for c in CASES)
    summary=f"{len(runs)}/36 runs available. Cases: {counts['accepted']} accepted, {counts['targets not met']} targets not met, {counts['unresolved']} unresolved."
    body=root.find(tag(W,'body'));tables=body.findall(tag(W,'tbl'))
    def table(index,rows):
        t=tables[index];existing=t.findall(tag(W,'tr'))
        while len(existing)<len(rows):t.append(copy.deepcopy(existing[-1]));existing=t.findall(tag(W,'tr'))
        for row,values in zip(existing,rows):
            for cell,v in zip(row.findall(tag(W,'tc')),values):
                ps=cell.findall(tag(W,'p'));paragraph(ps[0],v)
                for p in ps[1:]:paragraph(p,'')
    table(0,[['Version','1.0'],['Prepared By','Automated local engineering validation'],['Reviewers','Not assigned'],['Date Prepared',date],['Reporting Window','Fixed benchmark schedule; see run manifest'],['Status','Measured engineering report' if runs else 'Not measured']])
    table(1,[['Field','Details'],['Experiment Name','Phase-space / live dialectic renderer'],['Experiment Key','phase-space-v1'],['Owner Team','NeuralCompose'],['Business Owner','Not specified'],['Product Surface','Linux native Rust GPU window'],['Primary Objective','Measure scene-submission timing and preserve signal/model semantics'],['Control (Variant A)','Trace-only rendering'],['Treatment (Variant B)','Trace, band atmosphere, chronological semantic graph, final text'],['Allocation','Three rounds; every case once per round'],['Unit of Randomization','Recorded seeded order within round'],['Audience','Engineering maintainers; no human participants'],['Exclusions','Dirty, incomplete or aborted runs are non-quotable; no replacements'],['Start Date',manifest.get('started_utc','Not measured')],['End Date',manifest.get('ended_utc','Not measured')],['Planned Runtime','36 × (5 s warmup + 60 s): 39 minutes, plus setup'],['Actual Runtime',fmt(sum(r['duration_s'] for r in runs))+' measured seconds' if runs else 'Not measured']])
    table(2,[['Metric','Target/Rule'],['Primary Metric','Per-run nearest-rank frame-interval p95 ≤20 ms'],['Guardrail 1','Per-run sample-to-submit age p95 ≤100 ms'],['Guardrail 2','Bounded buffers; no rendering-induced ingestion loss'],['Guardrail 3','Observer parity under a full publisher channel; absence never becomes similarity zero'],['Decision Rule','Three quotable repeats: 3/3 accepted; 0/3 not met; mixed or missing unresolved. No extra runs.']])
    table(3,[['Metric','Variant A (Control)','Variant B (Treatment)'],['Completed runs',str(len(a)),str(len(b))],['Rendered frames',str(sum(r['rendered_frames'] for r in a)),str(sum(r['rendered_frames'] for r in b))],['Accepted samples',str(sum(r['accepted_samples'] for r in a)),str(sum(r['accepted_samples'] for r in b))],['Cases × repeats','6 × 3 planned','6 × 3 planned'],['Sample Ratio Check','Not applicable; fixed engineering schedule','Not applicable; fixed engineering schedule']])
    x,y=stat(a,'frame_p95_ms'),stat(b,'frame_p95_ms')
    table(4,[['Measure','Variant A','Variant B','Absolute Delta','Relative Delta'],['Worst run-level frame p95 (ms)',fmt(x),fmt(y),delta(x,y),'Not a pooled percentile'],['95% Confidence Interval','Not applicable','Not applicable','Descriptive repeats','Not applicable'],['Two-Sided p-value','Not applicable','Not applicable','No inferential test','Not applicable'],['Outcome Narrative','Trace-only','Full composition',summary,'This host/build only']])
    table(5,[['Guardrail Metric','Variant A','Variant B','Delta','Status'],['Worst sample-to-submit p95 ms',fmt(stat(a,'age_p95_ms')),fmt(stat(b,'age_p95_ms')),delta(stat(a,'age_p95_ms'),stat(b,'age_p95_ms')),'Per-run target ≤100 ms'],['Maximum clipped display fraction',fmt(stat(a,'clipped_fraction')),fmt(stat(b,'clipped_fraction')),'Descriptive','Impulse fixture intentionally exceeds scale'],['Observer / drift parity','Deterministic test suite','Full-channel barrier test','No model/log change permitted','See validation transcript'],['Signal gap / missing similarity','Deterministic tests','Deterministic tests','No invented continuity or edges','See validation transcript']])
    segment=[['Segment','Control Value','Treatment Value','Absolute Delta','Interpretation']]
    for fixture,mapping in itertools.product(['rhythmic','noise','impulse'],['channels','delay']):
        ca=f'{fixture}-{mapping}-trace';cb=f'{fixture}-{mapping}-full';sa=spread(groups.get(ca,[]),'frame_p95_ms');sb=spread(groups.get(cb,[]),'frame_p95_ms')
        show=lambda s:'Not measured' if s is None else ' / '.join(fmt(v) for v in s)
        segment.append([f'{fixture}/{mapping}',show(sa),show(sb),delta(sa[1] if sa else None,sb[1] if sb else None),f"A: {verdict(groups.get(ca,[]))}; B: {verdict(groups.get(cb,[]))}"])
    table(6,segment)
    table(7,[['Item','Decision'],['Final Decision',summary],['Decision Date',date],['Approvers','Not assigned; this report records measurements, not approval'],['Rollout Type','Separate opt-in visualization shell; no model-policy change'],['Rollback Trigger','Any observer parity, source freshness, or provenance violation'],['Follow Up','Unresolved cases remain unresolved. Any follow-up requires a new protocol.']])
    sections={
        'Purpose':['Measure the rendering cost of the full composition against trace-only rendering on identical deterministic input.','Keep per-run evidence, build identity, and limits available to maintainers.'],
        'Business Context':['The scene combines four-channel EEG, frequency contributions, model candidates, and final text.','Semantic position encodes chronology and role only. Explicit edges carry cosine; absent embeddings do not produce invented coordinates or edges.'],
        'Design Notes':['Three complete rounds cover twelve cases. Each fresh process warms for five seconds and measures for sixty. Order is shuffled with a recorded fixed seed.','Both variants receive identical synthetic EEG and semantic events. This tests engineering behavior, not human perception or biological efficacy.'],
        'Hypothesis':['The full composition can satisfy the registered submission-cadence and receive-to-submit targets while preserving the core and dialectic contracts.'],
        'Success Criteria':['Apply both timing targets separately to every run. Only three successful quotable repeats accept a case; mixed results remain unresolved.'],
        'Pre-Launch Validation':['Require a clean committed checkout and release executable from that checkout.','Freeze the schedule, fixtures, variants, and decision rule before recorded execution.','Capture the executable digest, GPU/driver, actual scene extent, display configuration, and per-run timing arrays.','Keep dirty or incomplete runs non-quotable; never replace them.','Inspect preflight images and run deterministic tests, including forced publisher backpressure, before freezing the build.'],
        'Sample and Exposure Summary':[summary],
        'Primary Outcome':['The primary table reports the worst run-level p95 in each variant, not a pooled p95. Case spreads below retain the three-repeat variation.'],
        'Guardrail Metrics':['Timing thresholds apply per run. Other rows identify deterministic invariants and descriptive signal properties; they do not imply physiological validation.'],
        'Segment Results':['The six fixture/mapping groups were specified in advance. Each table entry is min / median / max of three run-level frame p95 values, in milliseconds.','Report every case even when targets fail. No extra runs resolve a mixed outcome.'],
        'Data Quality and Limitations':['Desktop scheduling and thermal/load conditions can affect frame timing. The recorded host configuration bounds the claim.','No human allocation, engagement, retention, or cognitive-state outcome was measured.','Capture incomplete runs in the failures file; keep them unresolved and unreplaced.','Only Linux on the named GPU/driver is measured. No other support-matrix row is promoted.','Sample-to-submit age begins at viewer receipt and ends after scene queue submission; it is not sensor-to-screen latency.','Historical text logs have no embeddings. They cannot reconstruct spatial similarity or a missing model trajectory.'],
        'Interpretation':[summary,'The result supports only the reported engineering decision for this host/build. It does not establish a universal performance guarantee or a cognitive intervention.'],
        'Post Test Actions':['Retain all run files and the registered schedule.','Use the dashboard to inspect each case and all three repetitions.','Keep mixed outcomes unresolved. Specify any follow-up before collecting it.','Preserve observer parity when extending the live feed.','Archive the build digest, source revision, template hashes, rendered previews, and validation transcript together.'],
        'Appendix A: Metric Definitions':['Frame interval: monotonic milliseconds between phase-scene GPU queue submissions; nearest-rank p95 per run.','Sample-to-submit age: submission time minus receive time of the newest accepted EEG frame.','FPS: observed scene-submission intervals divided by their elapsed measurement span.','Live fraction: submissions marked Live divided by submissions in the measurement span. Clipping fraction counts out-of-range vertices across rendered windows.','Case verdict: three quotable runs required; 3/3 meeting both targets accepted, 0/3 not met, mixed or incomplete unresolved.'],
        'Appendix B: Analyst Notes':['Use descriptive min/median/max of run-level p95 values; no pooled p95 or inferential significance test.','The registered count is fixed at three runs per case; no optional stopping, replacement, or extension runs.',f'Build revision: {commit}. Binary identity and hardware are in Runs and the benchmark manifest.','Original source templates are unchanged; hashes accompany the artifacts. Missing measurements and unassigned reviewers remain explicit.'],
    }
    section=None;offset=collections.Counter()
    cover={'Experiment Report':'Phase-space validation','Experiment ':'Engineering','Report Name':'Phase-space / dialectic','Author':'NeuralCompose','Month DD, YYYY':date}
    for p in body.findall(tag(W,'p')):
        t=text(p)
        if t in sections:section=t
        elif t in cover:paragraph(p,cover[t])
        elif t.startswith('['):
            values=sections.get(section,['Not measured']);index=offset[section];offset[section]+=1
            paragraph(p,values[index] if index<len(values) else 'See the registered protocol and retained per-run evidence.')
        if t=='Business Context':paragraph(p,'Engineering Context')
    age_heading=E.Element(tag(W,'p'));paragraph(age_heading,'Per-case sample-to-submit p95 spread (ms; min / median / max)')
    body.insert(len(body)-1,age_heading)
    for case in CASES:
        values=spread(groups.get(case,[]),'age_p95_ms')
        p=E.Element(tag(W,'p'));paragraph(p,case+': '+('Not measured' if values is None else ' / '.join(fmt(v) for v in values)))
        body.insert(len(body)-1,p)
    zip_write(source,out,{'word/document.xml':xml_bytes(root)})

def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--runs',type=pathlib.Path,required=True);p.add_argument('--output',type=pathlib.Path,required=True)
    p.add_argument('--dashboard-template',type=pathlib.Path,default=ROOT/'artifact-template-analytics-dashboard/assets/reference.xlsx')
    p.add_argument('--report-template',type=pathlib.Path,default=ROOT/'artifact-template-experiment-analysis/assets/reference.docx')
    a=p.parse_args();a.output.mkdir(parents=True,exist_ok=True)
    runs=[]
    for path in sorted(a.runs.glob('*.json')):
        obj=json.loads(path.read_text())
        if isinstance(obj,dict) and obj.get('schema')=='neuralcompose.phase-space.run.v1':runs.append(obj)
    groups=collections.defaultdict(list)
    for r in runs:groups[r['case']].append(r)
    manifest=json.loads((a.runs/'manifest.json').read_text()) if (a.runs/'manifest.json').exists() else {}
    workbook(a.dashboard_template,a.output/'phase-space-dashboard.xlsx',groups,runs,manifest)
    document(a.report_template,a.output/'phase-space-experiment.docx',groups,runs,manifest)
    result=dict(cases={case:dict(verdict=verdict(groups[case]),frame_p95_spread=spread(groups[case],'frame_p95_ms'),age_p95_spread=spread(groups[case],'age_p95_ms'),runs=len(groups[case])) for case in CASES},
        template_sha256={p.name+' '+p.parent.parent.name:hashlib.sha256(p.read_bytes()).hexdigest() for p in [a.dashboard_template,a.report_template]},commit=manifest.get('commit'),run_count=len(runs))
    (a.output/'summary.json').write_text(json.dumps(result,indent=2)+'\n')
    print(json.dumps({'runs':len(runs),'verdicts':dict(collections.Counter(c['verdict'] for c in result['cases'].values()))}))
if __name__=='__main__':main()
