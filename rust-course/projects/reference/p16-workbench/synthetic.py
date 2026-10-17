#!/usr/bin/env python3
"""Local deterministic scenario. Start the Rust service first. Uses public demo fixture identity."""
import json,urllib.request,time
BASE='http://127.0.0.1:4116'
def post(path,body):
 data=json.dumps(body).encode();r=urllib.request.Request(BASE+path,data,headers={'Content-Type':'application/json','Authorization':'Bearer demo-operator'})
 with urllib.request.urlopen(r,timeout=3) as response: print(response.status,response.read().decode())
for sequence,price in enumerate([12345,12350,12320,12380],1):
 post('/api/ticks',dict(event_id=f'synthetic-{sequence}',symbol='RUST',price_cents=price,sequence=sequence))
 time.sleep(.2)
post('/api/orders',dict(order_id='paper-buy-1',symbol='RUST',side='buy',quantity=2))
post('/api/orders',dict(order_id='paper-buy-1',symbol='RUST',side='buy',quantity=2))
