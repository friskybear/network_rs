import socket
import network
import asyncio
from concurrent.futures import ThreadPoolExecutor

executor = ThreadPoolExecutor(max_workers=5)


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


def hi(str: str):
    print("from python " + str)
    return (str+"10")


key = network.get_key()


def add_ten(str: str) -> str:
    return str + "10"


async def receiver(key, port, n):
    await network.receive(key, port, n, add_ten)


async def send(key, port, list, str=""):
    await network.send(key, port, str, list)


async def get_user_input():
    loop = asyncio.get_running_loop()
    user_input = await loop.run_in_executor(None, input, "Please enter something: ")
    return user_input


async def main():
    key = [8, 1, 18, 64, 50, 162, 66, 5, 231, 230, 93, 219, 254, 23, 60, 84, 233, 200, 117, 244, 182, 2, 2, 172, 9, 167, 126, 53, 225, 123, 8, 170, 116, 88,
           199, 63, 54, 132, 65, 88, 202, 101, 105, 24, 164, 38, 16, 125, 87, 81, 255, 97, 42, 232, 167, 127, 141, 37, 53, 143, 183, 52, 177, 47, 98, 130, 79, 139]
    port = find_free_port()+2
    task1 = asyncio.create_task(receiver(key, port, 2))
    # ip = await get_user_input()
    # key = network.get_key()
    # port = find_free_port()
    # task2 = asyncio.create_task(send(key,port,[ip],""))

    await task1  # wait for my_task to complete
    # await task2

asyncio.run(main())
