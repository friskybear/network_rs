import network
import asyncio
from concurrent.futures import ThreadPoolExecutor

executor = ThreadPoolExecutor(max_workers=5)

import socket

def find_free_port(start_port=10001, end_port=65535):
    """Finds a free port between start_port and end_port."""
    for port in range(start_port, end_port + 1):
        with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
            try:
                s.bind(("0.0.0.0", port))
                return port
            except OSError:
                continue
    raise RuntimeError("No free port found in the specified range.")
port = find_free_port()
def hi(str:str):
    print("from python " + str)
    return(str+"10")

key = network.get_key();

def add_ten(str:str)->str:
    return str + "10"
async def receiver(key,port,n):
    await network.receive(key,port,n,add_ten)

async def send(key,port,list,str=""):
    await network.send(key,port,str,list)
    
    
async def get_user_input():
    loop = asyncio.get_running_loop()
    user_input = await loop.run_in_executor(None, input, "Please enter something: ")
    return "/ip4/127.0.0.1/udp/10003/quic-v1/p2p/12D3KooWDVB9kceXRU9fKR5LokzASPpqSnWEYqxZnpmBwixHQiaN"
async def main():
    ip = await get_user_input()
    key = network.get_key()
    port = find_free_port()+3
    res = await asyncio.create_task(send(key,port,[ip],""))
    print(res)
    
    

asyncio.run(main())