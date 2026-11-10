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
    finally:
        server.kill(); server.wait()
