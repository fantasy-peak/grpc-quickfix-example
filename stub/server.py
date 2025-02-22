import quickfix as fix

class FixServer(fix.Application):
    def onCreate(self, sessionID):
        print(f"[SERVER] Session created: {sessionID}")

    def onLogon(self, sessionID):
        print(f"[SERVER] Client logged in: {sessionID}")

    def onLogout(self, sessionID):
        print(f"[SERVER] Client logged out: {sessionID}")

    def toAdmin(self, message, sessionID):
        print(f"[SERVER] Sending Admin Message: {message}")

    def fromAdmin(self, message, sessionID):
        print(f"[SERVER] Received Admin Message: {message}")

    def toApp(self, message, sessionID):
        print(f"[SERVER] Sending App Message: {message}")

    def fromApp(self, message, sessionID):
        print(f"[SERVER] Received App Message: {message}")
        self.handleMessage(message, sessionID)

    def handleMessage(self, message, sessionID):
        msg_type = fix.MsgType()
        message.getHeader().getField(msg_type)

        if msg_type.getValue() == fix.MsgType_NewOrderSingle:  # 处理订单请求
            print("[SERVER] Received New Order Single")

            # 获取订单 ID
            order_id = fix.ClOrdID()
            message.getField(order_id)

            # 获取交易的 Symbol
            symbol = fix.Symbol()
            message.getField(symbol)

            # 获取买卖方向 (Side)
            side = fix.Side()
            message.getField(side)

            print(f"[SERVER] Order ID: {order_id.getValue()}, Symbol: {symbol.getValue()}, Side: {side.getValue()}")

            # **构造 Execution Report (8)**
            execution_report = fix.Message()
            execution_report.getHeader().setField(fix.MsgType(fix.MsgType_ExecutionReport))
            
            execution_report.setField(fix.OrderID("12345"))  # 订单编号
            execution_report.setField(order_id)  # 客户订单编号
            execution_report.setField(symbol)  # 交易的 Symbol
            execution_report.setField(fix.ExecID("54321"))  # 执行编号
            execution_report.setField(fix.ExecType(fix.ExecType_FILL))  # 订单执行类型
            execution_report.setField(fix.OrdStatus(fix.OrdStatus_FILLED))  # 订单状态：已成交
            execution_report.setField(side)  # 买卖方向
            execution_report.setField(fix.LeavesQty(0))  # 剩余未成交数量
            execution_report.setField(fix.CumQty(100))  # 累积成交数量
            execution_report.setField(fix.AvgPx(150.25))  # 平均成交价

            # 发送执行报告
            fix.Session.sendToTarget(execution_report, sessionID)
            print("[SERVER] Execution Report Sent")

def main():
    settings = fix.SessionSettings("quickfix_server.cfg")
    application = FixServer()
    store_factory = fix.FileStoreFactory(settings)
    log_factory = fix.FileLogFactory(settings)
    acceptor = fix.SocketAcceptor(application, store_factory, settings, log_factory)

    print("[SERVER] FIX Server started...")
    acceptor.start()

    try:
        while True:
            pass
    except KeyboardInterrupt:
        print("\n[SERVER] Shutting down...")
        acceptor.stop()

if __name__ == "__main__":
    main()
