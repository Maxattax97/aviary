import json, os, pty, select, socket, subprocess, tempfile, termios, time
binary = os.path.abspath('target/debug/aviary')
sock = socket.socket(); sock.bind(('127.0.0.1', 0)); port = sock.getsockname()[1]; sock.close()
address = f'127.0.0.1:{port}'
with tempfile.TemporaryDirectory() as d:
    server = subprocess.Popen([binary, 'serve', d + '/db', address], stderr=subprocess.DEVNULL)
    def call(op):
        return json.loads(subprocess.check_output([binary, 'call', address, json.dumps(op)], stderr=subprocess.DEVNULL, timeout=15))
    try:
        for _ in range(100):
            try:
                with socket.create_connection(('127.0.0.1', port), timeout=.1): break
            except OSError: time.sleep(.02)
        for case in ('recommended','other','custom'):
            request = {'identity':case,'agent':'agent','question':'Choose '+case,'options':['No','Yes'],'recommendation':1,'reason':'Ready'}
            if case == 'custom': request = {'identity':case,'agent':'agent','question':'Choose custom','options':[]}
            ident = call({'op':'submit','request':request})['id']
            master, slave = pty.openpty(); before = termios.tcgetattr(slave)
            proc = subprocess.Popen([binary, 'tui', address], stdin=slave, stdout=slave, stderr=slave)
            output = b''; deadline = time.monotonic() + 8
            while ('Choose '+case).encode() not in output and time.monotonic() < deadline:
                if select.select([master],[],[],.2)[0]: output += os.read(master,65536)
            assert ('Choose '+case).encode() in output, output
            assert call({'op':'get','id':ident})['answer'] is None
            if case == 'recommended': os.write(master,b'\r')
            elif case == 'other': os.write(master,b'2\r')
            else: os.write(master,'j\rCustom 鳥\r'.encode())
            deadline = time.monotonic() + 8
            while time.monotonic() < deadline:
                answer = call({'op':'get','id':ident})['answer']
                if answer is not None: break
                time.sleep(.05)
            expected = {'recommended':{'type':'suggested','index':1,'value':'Yes'},'other':{'type':'suggested','index':0,'value':'No'},'custom':{'type':'custom','value':'Custom 鳥'}}[case]
            assert answer == expected, (case,answer)
            os.write(master,b'q'); proc.wait(timeout=5)
            assert proc.returncode == 0
            assert termios.tcgetattr(slave) == before
            os.close(master); os.close(slave)
            print(case, 'passed; terminal restored')
        request = {'identity':'deferred','agent':'agent','question':'Defer this','options':['Yes'],'recommendation':0,'reason':'Ready','importance':1}
        ident = call({'op':'submit','request':request})['id']
        master, slave = pty.openpty(); before = termios.tcgetattr(slave)
        proc = subprocess.Popen([binary, 'tui', address], stdin=slave, stdout=slave, stderr=slave)
        def wait_output(marker):
            output = b''; deadline = time.monotonic() + 8
            while marker not in output and time.monotonic() < deadline:
                if select.select([master],[],[],.2)[0]: output += os.read(master,65536)
            assert marker in output, output
            return output
        def wait_ids(op, ids):
            deadline = time.monotonic() + 8
            while time.monotonic() < deadline:
                if [r['id'] for r in call({'op':op})['requests']] == ids: return
                time.sleep(.05)
            raise AssertionError((op, ids))
        wait_output(b'Defer this')
        os.write(master,b'd'); wait_ids('list_deferred',[ident])
        os.write(master,b'q'); proc.wait(timeout=5)
        assert termios.tcgetattr(slave) == before
        os.close(master); os.close(slave)
        master, slave = pty.openpty(); before = termios.tcgetattr(slave)
        proc = subprocess.Popen([binary, 'tui', address], stdin=slave, stdout=slave, stderr=slave)
        wait_output(b'No active requests')
        os.write(master,b'\t'); wait_output(b'Defer this')
        os.write(master,b'p5')
        deadline = time.monotonic() + 8
        while time.monotonic() < deadline:
            items = call({'op':'list_deferred'})['requests']
            if items and items[0]['importance'] == 5: break
            time.sleep(.05)
        else: raise AssertionError('priority override not stored')
        os.write(master,b'd'); wait_ids('list',[ident])
        os.write(master,b'q'); proc.wait(timeout=5)
        assert proc.returncode == 0 and termios.tcgetattr(slave) == before
        os.close(master); os.close(slave)
        print('defer, reopen, priority override, restore passed; terminal restored')
        call({'op':'answer','id':ident,'index':0})
        details = {'identity':'details','agent':'agent','question':'Detail check','options':['Yes'],'recommendation':0,'reason':'Ready','explanation':'Why this matters','context':'A'*430 + ' END-CONTEXT'}
        detail_id = call({'op':'submit','request':details})['id']
        master, slave = pty.openpty(); before = termios.tcgetattr(slave)
        proc = subprocess.Popen([binary, 'tui', address], stdin=slave, stdout=slave, stderr=slave)
        wait_output(b'Why this matters')
        os.write(master,b'c'); wait_output(b'page 1/')
        os.write(master,b'jj'); wait_output(b'END-CONTEXT')
        os.write(master,b'q'); wait_output(b'Detail check')
        assert call({'op':'get','id':detail_id})['answer'] is None
        os.write(master,b'q'); proc.wait(timeout=5)
        assert proc.returncode == 0 and termios.tcgetattr(slave) == before
        os.close(master); os.close(slave)
        print('explanation and paged context passed; no answer submitted')
        master, slave = pty.openpty(); before = termios.tcgetattr(slave)
        proc = subprocess.Popen([binary, 'tui', address], stdin=slave, stdout=slave, stderr=slave)
        wait_output(b'Detail check')
        notice = {'identity':'notice-pty','agent':'worker','work':'Build package','result':'Tests passed'}
        notice_id = call({'op':'submit_notice','notice':notice})['id']
        wait_output(b'Open completion notices: 1')
        os.write(master,b'n'); assert b'Tests passed' in wait_output(b'Build package')
        os.write(master,b'd')
        deadline = time.monotonic() + 8
        while time.monotonic() < deadline:
            if call({'op':'list_notices'})['notices'] == []: break
            time.sleep(.05)
        else: raise AssertionError('notice not dismissed')
        assert call({'op':'list_notices','include_dismissed':True})['notices'][0]['id'] == notice_id
        os.write(master,b'q'); wait_output(b'Detail check')
        os.write(master,b'q'); proc.wait(timeout=5)
        assert proc.returncode == 0 and termios.tcgetattr(slave) == before
        os.close(master); os.close(slave)
        print('live completion notice displayed and dismissed')
        call({'op':'create_channel','channel':{'name':'ops','members':['operator','a','b']}})
        master, slave = pty.openpty(); before = termios.tcgetattr(slave)
        proc = subprocess.Popen([binary, 'tui', address], stdin=slave, stdout=slave, stderr=slave)
        wait_output(b'Detail check')
        for identity, target, text in [('instruction-direct','a','Review this'),('instruction-group','','Start work')]:
            os.write(master,b'm'); wait_output(b'Retry identity')
            os.write(master,(identity+'\r'+'ops'+'\r'+target+'\r'+text+'\r').encode())
            review = wait_output(b'Review instruction')
            assert identity.encode() in review and text.encode() in review
            os.write(master,b'\r'); wait_output(b'Instruction accepted')
        os.write(master,b'm'); wait_output(b'Retry identity')
        os.write(master,b'instruction-invalid\rmissing-channel\ra\rHold\r')
        wait_output(b'Review instruction'); os.write(master,b'\r')
        failed = wait_output(b'Send failed or outcome unknown')
        assert b'instruction-invalid' in failed and b'Hold' in failed
        os.write(master,b'\r'); wait_output(b'Send failed or outcome unknown')
        os.write(master,b'\x1b'); wait_output(b'Detail check')
        inbox_a = call({'op':'inbox','participant':'a'})['messages']
        inbox_b = call({'op':'inbox','participant':'b'})['messages']
        assert [m['message']['body'] for m in inbox_a] == ['Review this','Start work']
        assert [m['message']['body'] for m in inbox_b] == ['Start work']
        assert all(m['message']['sender'] == 'operator' for m in inbox_a)
        os.write(master,b'q'); proc.wait(timeout=5)
        assert proc.returncode == 0 and termios.tcgetattr(slave) == before
        os.close(master); os.close(slave)
        print('operator direct and group instructions delivered')
    finally:
        server.kill(); server.wait()
