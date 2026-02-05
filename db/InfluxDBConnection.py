import logging
import time
import threading
from datetime import datetime, timedelta
from influxdb import InfluxDBClient

class InfluxDBConnectionHandler:
    def __init__(self, host, port, username, password, database, timeout, retry_interval, fetch_interval):
        self.host = host
        self.port = port
        self.username = username
        self.password = password
        self.database = database
        self.timeout = timeout
        self.retry_interval = retry_interval
        self.fetch_interval = fetch_interval
        self.client = None
        self.connection_thread = threading.Thread(target=self.connect_to_influxdb)
        self.connection_thread.daemon = True
        self.fetch_thread = threading.Thread(target=self.fetch_latest_record)
        self.fetch_thread.daemon = True
        self.exec = 0
        self.node = 0
        self.cons = 0

    def connect_to_influxdb(self):
        current_attempt = 0
        while self.client is None:
            try:
                client = InfluxDBClient(host=self.host, port=self.port, username=self.username, password=self.password,
                                        database=self.database, timeout=self.timeout)
                if client.ping():
                    logging.info("InfluxDB: Connection successful!")
                    self.client = client
                else:
                    logging.warning("InfluxDB: Connection failed: ping unsuccessful")
                    self.client = None
            except Exception as e:
                try:
                    logging.error(f"InfluxDB: An error occurred: {e}")
                except:
                    logging.error("InfluxDB: An error occurred while logging error")
                self.client = None

            if self.client is None:
                delay = int(min(300, self.retry_interval * (1.5 ** current_attempt)))
                logging.info(f"InfluxDB: Retrying connection in {delay} seconds...")
                time.sleep(delay)
                current_attempt += 1

    def start(self):
        self.connection_thread.start()
        self.fetch_thread.start()

    def get_client(self):
        return self.client

    def get_exec_status(self):
        return self.exec

    def get_node_status(self):
        return self.node

    def get_cons_status(self):
        return self.cons

    def fetch_latest_record(self):
        time.sleep(3)
        current_attempt = 0
        while True:
            if self.client is None:
                delay = int(min(300, self.retry_interval * (1.5 ** current_attempt)))
                logging.info(f"InfluxDB: Client is not connected, retrying connection in {delay} seconds...")
                time.sleep(delay)
                current_attempt += 1
                continue

            current_attempt = 0
            try:
                    result1 = self.client.query(f'SELECT "active_percent" FROM "status_exec" WHERE "host"::tag =~ /^{self.host}_s$/ ORDER BY time DESC LIMIT 1')
                    points1 = list(result1.get_points())
                    if points1:
                        self.exec = points1[0]['active_percent']

                    result2 = self.client.query(
                        f'SELECT "active_percent" FROM "status_node" WHERE "host"::tag =~ /^{self.host}_s$/ ORDER BY time DESC LIMIT 1')
                    points2 = list(result2.get_points())
                    if points2:
                        self.node = points2[0]['active_percent']

                    result3 = self.client.query(
                        f'SELECT "active_percent" FROM "status_consensus" WHERE "host"::tag =~ /^{self.host}_s$/ ORDER BY time DESC LIMIT 1')
                    points3 = list(result3.get_points())
                    if points3:
                        self.cons = points3[0]['active_percent']

                    # logging.info(f'InfluxDB: {self.exec} / {self.node} / {self.cons}')

                    time.sleep(self.fetch_interval)

            except Exception as e:
                try:
                    logging.error(f'InfluxDB: An error occurred while fetching the latest record: {str(e)}')
                except:
                    logging.error("InfluxDB: An error occurred while logging error")
                self.client = None
                self.exec = 0
                self.node = 0
                self.cons = 0
                self.connection_thread = threading.Thread(target=self.connect_to_influxdb)
                self.connection_thread.start()
                time.sleep(3)