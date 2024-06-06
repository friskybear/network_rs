import random
import time
import network_rs
import asyncio
from concurrent.futures import ThreadPoolExecutor
import sys
import json
from frost_rs import utility_secp256k1 as frost

def coordinate(ip: str, _: list[str]):
    ip = json.loads(ip)
    if ip['protocol'] == 'coordinate':
        return json.dumps({'protocol': 'coordinate', 'address': ip['address'], 'status': 'Success'})
    else:
        return json.dumps({'protocol': 'coordinate', 'status': 'Failed'})


async def main():

    _key = network_rs.get_key()
    port = network_rs.get_free_port()
    num_of_nodes: int = int(sys.argv[1])
    min_signers: int = int(sys.argv[2])
    max_signers: int = int(sys.argv[3])
    number_of_signature: int = int(sys.argv[4])
    result = await network_rs.receive(_key, port, num_of_nodes, coordinate, [], [])
    nodes: dict = json.loads(result)
    nodes_ip = []

    for _, value in nodes.items():
        address = value['address']
        if address:
            nodes_ip.append(address)
    random.sample(nodes_ip, max_signers)
    round1_protocol = {'protocol': 'round1',
                       'min_signers': min_signers, 'max_signers': max_signers}
    then = time.time()
    round1_result = await network_rs.send(_key, port, json.dumps(round1_protocol), nodes_ip)
    round1_public_packages = {}
    for _, content in json.loads(round1_result).items():
        message = json.loads(content['message'])
        round1_public_packages[message["id"]
                               ] = message["round1_public_package"]

    round2_protocol = {'protocol': 'round2',
                       'round1_received_packages': json.dumps(round1_public_packages)}
    round2_result = await network_rs.send(_key, port, json.dumps(round2_protocol), nodes_ip)
    round2_public_packages = {}
    for _, content in json.loads(round2_result).items():
        message = json.loads(content['message'])
        round2_public_packages[message['id']
                               ] = message['round2_public_package']

    round3_protocol = {'protocol': 'round3',
                       'round2_received_packages': round2_public_packages}
    round3_result = await network_rs.send(_key, port, json.dumps(round3_protocol), nodes_ip)
    public_key: str = json.loads(list(json.loads(round3_result).values())[
        0]['message'])['public_key']
    generate_nonce_protocol = {
        'protocol': 'generate_nonce', 'num_of_nonces': number_of_signature}
    min_ip = random.sample(nodes_ip, min_signers)
    generate_nonce_result = await network_rs.send(_key, port, json.dumps(generate_nonce_protocol), min_ip)
    nonces = {}
    for key, content in json.loads(generate_nonce_result).items():
        peer_id = network_rs.get_peer_id_from_address(key)
        nonces[frost.get_id(peer_id)] = json.loads(
            content['message'])['public_nonces']
    signatures = []
    message = 'hello from frost_rs'
    for i in range(0, number_of_signature):
        sub_commitment = {k: v[i] for k, v in nonces.items()}
        signature_protocol = {'protocol': 'sign', 'signature_id': i,
                              'message': message, 'commitments': sub_commitment}
        result = await network_rs.send(_key, port, json.dumps(signature_protocol), min_ip)
        shares = {}
        for key, content in json.loads(result).items():
            peer_id = network_rs.get_peer_id_from_address(key)
            shares[frost.get_id(peer_id)] = json.loads(
                content['message'])['share']
        signature = frost.aggregate(
            bytes(message, "utf-8"), sub_commitment, shares, public_key)

        if frost.verify(bytes(message, "utf-8"), public_key, signature):
            signatures.append(signature)
    print(f'\nsignatures:{signatures}\n')
    print(time.time()-then, 's')
    exit(0)

asyncio.run(main())
