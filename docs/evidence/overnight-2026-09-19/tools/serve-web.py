"""Static server for web/ with a listen backlog large enough for Chrome's parallel requests.
python3 -m http.server keeps the default backlog of 5 and resets the overflow."""
import functools, http.server, sys
root, port = sys.argv[1], int(sys.argv[2])
class Server(http.server.ThreadingHTTPServer):
    request_queue_size = 128
    daemon_threads = True
Server(("127.0.0.1", port), functools.partial(http.server.SimpleHTTPRequestHandler, directory=root)).serve_forever()
