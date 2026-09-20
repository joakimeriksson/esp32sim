const http = require('node:http');
const server = http.createServer((req, res) => {
  console.log(JSON.stringify({method:req.method, path:req.url, peer:req.socket.remoteAddress}));
  res.writeHead(200, {'Content-Type':'text/plain', 'Content-Length':27, Connection:'close'});
  res.end('esp32sim-nat-loopback-pass\n');
});
server.listen(18799, '127.0.0.1', () => console.log('listening 127.0.0.1:18799'));
setTimeout(() => server.close(), 180000);
