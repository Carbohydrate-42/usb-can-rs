//! X_V2步进闭环控制例程（Rust `no_std` 版本，重构自 X_V2.c / X_V2.h）
//!
//! 编写作者：ZHANGDATOU（原版 C 代码）
//! 技术支持：张大头闭环伺服
//! 淘宝店铺：https://zhangdatou.taobao.com
//! CSDN博客：http s://blog.csdn.net/zhangdatou666
//! qq交流群：262438510
//!
//! # 协议说明
//!
//! - 逻辑帧格式：`地址 + 功能码 + 数据 + 校验字节(0x6B)`，多字节整数均为大端
//! - 注释中带（X42S/Y42）的为 X42S/Y42 新增命令，X42 不能用，其他通用
//! - 命令构造与传输分离：各命令函数只负责拼帧，返回 [`Command`]；
//!   发送由 [`CanTx`]、接收由 [`CanRx`] 负责（两者都实现即自动获得 [`CanBus`] 的
//!   `write_read` 事务能力），多电机批处理由 [`Transaction`] 负责
//! - 另有一套平行的 async 接口：[`AsyncCanTx`] / [`AsyncCanRx`] / [`AsyncCanBus`]，
//!   供 embassy 等 async executor 场景使用（需要 Rust 1.75+）
//!
//! # 示例
//!
//! ```ignore
//! // 单条命令：拼帧 + 发送
//! can.send_cmd(en_control(1, true, false).as_bytes());
//!
//! // 查询应答
//! if let Some(resp) = can.write_read(read_sys_params(1, SysParams::Vel).as_bytes(), 1000) {
//!     // resp.data[..resp.len]
//! }
//!
//! // 多电机事务：载入多条子命令，一次性提交
//! let mut t = Transaction::begin();
//! t.load(en_control(1, true, false)).load(en_control(2, true, false));
//! if let Some(batch) = t.commit(0) {
//!     can.send_cmd(batch.as_bytes());
//! }
//! ```

/// 校验字节
const CHECK: u8 = 0x6B;

/// 多电机命令缓冲区长度
pub const MMCL_LEN: usize = 512;

/// 单条命令的最大长度
pub const MAX_CMD_LEN: usize = 32;

/// 一条完整的逻辑帧（`地址 + 功能码 + 数据 + 校验字节`）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Command {
    buf: [u8; MAX_CMD_LEN],
    len: usize,
}

impl Command {
    fn new<const M: usize>(bytes: [u8; M]) -> Self {
        debug_assert!(M <= MAX_CMD_LEN);
        let mut buf = [0u8; MAX_CMD_LEN];
        buf[..M].copy_from_slice(&bytes);
        Self { buf, len: M }
    }

    fn from_parts(buf: [u8; MAX_CMD_LEN], len: usize) -> Self {
        debug_assert!(len <= MAX_CMD_LEN);
        Self { buf, len }
    }

    /// 完整帧字节
    pub fn as_bytes(&self) -> &[u8] {
        &self.buf[..self.len]
    }

    /// 帧长度
    pub fn len(&self) -> usize {
        self.len
    }

    /// 帧是否为空
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
}

/// 多电机打包帧（0xAA 功能码，一条帧携带多条子命令）
#[derive(Debug, Clone)]
pub struct Batch {
    buf: [u8; MMCL_LEN + 5],
    len: usize,
}

impl Batch {
    /// 完整帧字节
    pub fn as_bytes(&self) -> &[u8] {
        &self.buf[..self.len]
    }

    /// 帧长度
    pub fn len(&self) -> usize {
        self.len
    }

    /// 帧是否为空
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
}

/// 多电机命令事务（对应原 C 的 MMCL 机制）
///
/// 用 `begin` 开启事务，`load` 载入若干条子命令（子命令自带电机地址），
/// 最后 `commit` 打包成一帧 0xAA 多电机命令，一次发送出去。
pub struct Transaction {
    buf: [u8; MMCL_LEN],
    count: usize,
}

impl Default for Transaction {
    fn default() -> Self {
        Self::begin()
    }
}

impl Transaction {
    /// 开启一个空事务
    pub fn begin() -> Self {
        Self {
            buf: [0; MMCL_LEN],
            count: 0,
        }
    }

    /// 载入一条子命令；缓冲区满时截断丢弃（原 C 版本无边界检查，会越界）
    pub fn load(&mut self, cmd: Command) -> &mut Self {
        let n = cmd.len().min(MMCL_LEN - self.count);
        self.buf[self.count..self.count + n].copy_from_slice(&cmd.buf[..n]);
        self.count += n;
        self
    }

    /// 打包为多电机命令帧；`addr` 一般用广播地址 0；空事务返回 `None`
    pub fn commit(self, addr: u8) -> Option<Batch> {
        if self.count == 0 {
            return None;
        }
        // 多电机命令的总字节数
        let total = (self.count + 5) as u16;

        let mut batch = Batch {
            buf: [0; MMCL_LEN + 5],
            len: self.count + 5,
        };
        batch.buf[0] = addr;
        batch.buf[1] = 0xAA;
        batch.buf[2..4].copy_from_slice(&total.to_be_bytes());
        batch.buf[4..4 + self.count].copy_from_slice(&self.buf[..self.count]);
        batch.buf[4 + self.count] = CHECK;
        Some(batch)
    }
}

/// 系统参数类型（对应 C 的 `SysParams_t`）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum SysParams {
    /// 读取总线电压
    VBus = 5,
    /// 读取总线电流
    CBus = 6,
    /// 读取相电流
    CPha = 7,
    /// 读取编码器原始值
    Enco = 8,
    /// 读取实时脉冲数
    Clkc = 9,
    /// 读取经过线性化校准后的编码器值
    Encl = 10,
    /// 读取输入脉冲数
    Clki = 11,
    /// 读取电机目标位置
    TPos = 12,
    /// 读取电机实时设定的目标位置
    SPos = 13,
    /// 读取电机实时转速
    Vel = 14,
    /// 读取电机实时位置
    CPos = 15,
    /// 读取电机位置误差
    PErr = 16,
    /// 读取多圈编码器电池电压（Y42）
    VBat = 17,
    /// 读取电机实时温度（X42S/Y42）
    Temp = 18,
    /// 读取电机状态标志位
    Flag = 19,
    /// 读取回零状态标志位
    OFlag = 20,
    /// 读取电机状态标志位 + 回零状态标志位（X42S/Y42）
    Oaf = 21,
    /// 读取引脚状态（X42S/Y42）
    Pin = 22,
    /// 读取系统状态信息
    Sys = 23,
}

impl SysParams {
    /// 「读取系统参数」命令对应的功能码；
    /// `Sys` 比较特殊，返回 `(0x43, Some(0x7A))`，需要多补一个辅助码字节
    fn code(self) -> (u8, Option<u8>) {
        use SysParams::*;
        match self {
            VBus => (0x24, None),
            CBus => (0x26, None),
            CPha => (0x27, None),
            Enco => (0x29, None),
            Clkc => (0x30, None),
            Encl => (0x31, None),
            Clki => (0x32, None),
            TPos => (0x33, None),
            SPos => (0x34, None),
            Vel => (0x35, None),
            CPos => (0x36, None),
            PErr => (0x37, None),
            VBat => (0x38, None),
            Temp => (0x39, None),
            Flag => (0x3A, None),
            OFlag => (0x3B, None),
            Oaf => (0x3C, None),
            Pin => (0x3D, None),
            Sys => (0x43, Some(0x7A)),
        }
    }

    /// 「定时返回信息」命令对应的信息功能码；
    /// 该命令不支持 `Sys`（C 版本 switch 中没有 S_SYS 分支，落入 default 不发信息码）
    fn timed_code(self) -> Option<u8> {
        match self {
            SysParams::Sys => None,
            s => Some(s.code().0),
        }
    }
}

/// 一帧接收到的 CAN 扩展帧
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CanFrame {
    /// 29 位扩展帧 ID（电机地址 = `ext_id >> 8`，分包序号 = `ext_id & 0xFF`）
    pub ext_id: u32,
    /// 帧数据
    pub data: [u8; 8],
    /// 数据长度（DLC）
    pub len: usize,
}

/// CAN 发送端抽象（由具体平台的 CAN 外设实现，替代 C 的 `can_SendCmd` 发送路径）
///
/// 只需实现 `send_frame`，逻辑帧的拆包由默认方法 `send_cmd` 完成。
/// trait 特意设计为非阻塞、不持有总线所有权，方便对接不同平台：
/// 同步外设直接实现即可；async 场景（如 embassy）可在任务/通道之上包装实现。
pub trait CanTx {
    /// 发送一帧 CAN 扩展帧（必须实现）
    ///
    /// * `ext_id` ：29 位扩展帧 ID
    /// * `data`   ：帧数据，长度不超过 8
    fn send_frame(&mut self, ext_id: u32, data: &[u8]);

    /// 发送一条完整命令帧（默认实现，一般无需覆写）
    ///
    /// `cmd` 为完整逻辑帧：`地址 + 功能码 + 数据 + 校验字节(0x6B)`，
    /// 按原 C 版本 `can_SendCmd` 的规则拆成 CAN 物理帧发送：
    /// - 扩展帧 ID：`ext_id = (cmd[0] << 8) | 分包序号`（电机地址在 ID 高字节）
    /// - 每帧数据：第 1 字节为功能码 `cmd[1]`，之后最多 7 字节数据
    /// - 命令超过 8 字节时分多包发送（每包 DLC ≤ 8）
    fn send_cmd(&mut self, cmd: &[u8]) {
        // 少于 2 字节（无地址/功能码）不是合法命令，直接忽略
        if cmd.len() < 2 {
            return;
        }

        // 除去ID地址和功能码后的数据长度
        let data_len = cmd.len() - 2;
        let mut i = 0;
        let mut pack_num = 0u32;

        // 大于8字节命令分包发送，每包数据最多发送8个字节
        while i < data_len {
            let k = (data_len - i).min(7);

            let ext_id = ((cmd[0] as u32) << 8) | pack_num;
            let mut frame = [0u8; 8];
            frame[0] = cmd[1]; // 功能码
            frame[1..1 + k].copy_from_slice(&cmd[2 + i..2 + i + k]);
            self.send_frame(ext_id, &frame[..1 + k]);

            i += k;
            pack_num += 1;
        }
    }
}

/// CAN 接收端抽象（由具体平台的 CAN 外设实现）
///
/// 非阻塞轮询式接口；async 场景下实现方可以在内部 await 通道/队列，
/// 或干脆由独立的 async 任务收帧后投递给本接口。
pub trait CanRx {
    /// 尝试接收一帧 CAN 扩展帧（无数据时返回 `None`）
    ///
    /// 返回 `(ext_id, data)`；电机应答帧的地址为 `ext_id >> 8`
    fn try_receive_frame(&mut self) -> Option<(u32, &[u8])>;
}

/// 同时具备收发能力的 CAN 总线（客户端-服务器模式需要）
///
/// 无需手动实现：任何同时实现 [`CanTx`] + [`CanRx`] 的类型
/// 会通过 blanket impl 自动获得本 trait 的默认方法。
pub trait CanBus: CanTx + CanRx {
    /// 客户端-服务器模式的事务：发送命令并等待目标电机的应答帧（默认实现）
    ///
    /// 类似 I2C 的 write-then-read：先发出 `cmd`，然后轮询 `try_receive_frame`，
    /// 返回**第一帧**地址匹配（`ext_id >> 8 == cmd[0]`）的应答；
    /// 其他来源的帧会被丢弃，注意总线上电机可能主动上报，应答不一定紧随命令。
    ///
    /// * `cmd`        ：完整逻辑帧
    /// * `poll_limit` ：最大轮询次数（no_std 下没有定时器，用它代替超时）
    ///
    /// 注意：只返回一帧，长应答（如读取系统状态参数）的多包重组需自行
    /// 继续调 `try_receive_frame` 按分包序号（`ext_id & 0xFF`）拼接；
    /// 应答 payload 的解析（功能码回显、数值大端等）也留给调用方。
    fn write_read(&mut self, cmd: &[u8], poll_limit: u32) -> Option<CanFrame> {
        if cmd.is_empty() {
            return None;
        }
        let addr = cmd[0] as u32;

        self.send_cmd(cmd);

        for _ in 0..poll_limit {
            if let Some((ext_id, data)) = self.try_receive_frame() {
                // 只认领目标电机发来的帧，其余帧丢弃
                if ext_id >> 8 == addr {
                    let len = data.len().min(8);
                    let mut frame_data = [0u8; 8];
                    frame_data[..len].copy_from_slice(&data[..len]);
                    return Some(CanFrame {
                        ext_id,
                        data: frame_data,
                        len,
                    });
                }
            }
        }
        None
    }
}

/// 任何同时实现 CanTx + CanRx 的类型自动获得 CanBus
impl<T: CanTx + CanRx> CanBus for T {}

/// 未实现的 CAN 占位实现（TODO：接入具体硬件后替换）
pub struct UnimplementedCan;

impl CanTx for UnimplementedCan {
    fn send_frame(&mut self, _ext_id: u32, _data: &[u8]) {
        unimplemented!("can_SendCmd 尚未移植，请为具体平台实现 CanTx trait")
    }
}

impl CanRx for UnimplementedCan {
    fn try_receive_frame(&mut self) -> Option<(u32, &[u8])> {
        unimplemented!("接收尚未移植，请为具体平台实现 CanRx trait")
    }
}

/**********************************************************
*** async 版本接口
***
*** 与上面的同步 trait 平行，语义一一对应，供 async executor
*** （embassy、tokio 上位机等）场景使用。需要 Rust 1.75+（async fn in trait）。
*** trait 不绑定任何 executor；超时不在本层处理，
*** 由调用方用 executor 的设施包裹（如 embassy_time::with_timeout）。
**********************************************************/

/// CAN 发送端抽象（async 版，与 [`CanTx`] 对应）
#[allow(async_fn_in_trait)]
pub trait AsyncCanTx {
    /// 发送一帧 CAN 扩展帧（必须实现）
    ///
    /// * `ext_id` ：29 位扩展帧 ID
    /// * `data`   ：帧数据，长度不超过 8
    async fn send_frame(&mut self, ext_id: u32, data: &[u8]);

    /// 发送一条完整命令帧（默认实现），拆包规则与 [`CanTx::send_cmd`] 相同
    async fn send_cmd(&mut self, cmd: &[u8]) {
        // 少于 2 字节（无地址/功能码）不是合法命令，直接忽略
        if cmd.len() < 2 {
            return;
        }

        // 除去ID地址和功能码后的数据长度
        let data_len = cmd.len() - 2;
        let mut i = 0;
        let mut pack_num = 0u32;

        // 大于8字节命令分包发送，每包数据最多发送8个字节
        while i < data_len {
            let k = (data_len - i).min(7);

            let ext_id = ((cmd[0] as u32) << 8) | pack_num;
            let mut frame = [0u8; 8];
            frame[0] = cmd[1]; // 功能码
            frame[1..1 + k].copy_from_slice(&cmd[2 + i..2 + i + k]);
            self.send_frame(ext_id, &frame[..1 + k]).await;

            i += k;
            pack_num += 1;
        }
    }
}

/// CAN 接收端抽象（async 版，与 [`CanRx`] 对应）
#[allow(async_fn_in_trait)]
pub trait AsyncCanRx {
    /// 等待并接收一帧 CAN 扩展帧（必须实现）
    ///
    /// 电机应答帧的地址为 `CanFrame::ext_id >> 8`
    async fn receive_frame(&mut self) -> CanFrame;
}

/// 同时具备收发能力的 async CAN 总线（客户端-服务器模式需要）
///
/// 无需手动实现：任何同时实现 [`AsyncCanTx`] + [`AsyncCanRx`] 的类型
/// 会通过 blanket impl 自动获得本 trait 的默认方法。
#[allow(async_fn_in_trait)]
pub trait AsyncCanBus: AsyncCanTx + AsyncCanRx {
    /// 客户端-服务器模式的事务：发送命令并等待目标电机的应答帧（默认实现）
    ///
    /// 发送 `cmd` 后持续 await 接收，返回**第一帧**地址匹配
    /// （`ext_id >> 8 == cmd[0]`）的应答，其他来源的帧会被丢弃。
    /// 没有内置超时：请用 executor 的设施包裹
    /// （如 `embassy_time::with_timeout`、`tokio::time::timeout`）。
    ///
    /// 注意：只返回一帧，长应答的多包重组和 payload 解析留给调用方。
    async fn write_read(&mut self, cmd: &[u8]) -> Option<CanFrame> {
        if cmd.is_empty() {
            return None;
        }
        let addr = cmd[0] as u32;

        self.send_cmd(cmd).await;

        loop {
            let frame = self.receive_frame().await;
            // 只认领目标电机发来的帧，其余帧丢弃
            if frame.ext_id >> 8 == addr {
                return Some(frame);
            }
        }
    }
}

/// 任何同时实现 AsyncCanTx + AsyncCanRx 的类型自动获得 AsyncCanBus
impl<T: AsyncCanTx + AsyncCanRx> AsyncCanBus for T {}

/// 未实现的 async CAN 占位实现（TODO：接入具体硬件后替换）
pub struct UnimplementedAsyncCan;

impl AsyncCanTx for UnimplementedAsyncCan {
    async fn send_frame(&mut self, _ext_id: u32, _data: &[u8]) {
        unimplemented!("can_SendCmd 尚未移植，请为具体平台实现 AsyncCanTx trait")
    }
}

impl AsyncCanRx for UnimplementedAsyncCan {
    async fn receive_frame(&mut self) -> CanFrame {
        unimplemented!("接收尚未移植，请为具体平台实现 AsyncCanRx trait")
    }
}

/**********************************************************
*** 触发动作命令
**********************************************************/

/// 触发编码器校准（原 `X_V2_Trig_Encoder_Cal`）
pub fn trig_encoder_cal(addr: u8) -> Command {
    Command::new([addr, 0x06, 0x45, CHECK])
}

/// 重启电机（X42S/Y42）（原 `X_V2_Reset_Motor`）
pub fn reset_motor(addr: u8) -> Command {
    Command::new([addr, 0x08, 0x97, CHECK])
}

/// 将当前位置清零（原 `X_V2_Reset_CurPos_To_Zero`）
pub fn reset_cur_pos_to_zero(addr: u8) -> Command {
    Command::new([addr, 0x0A, 0x6D, CHECK])
}

/// 解除堵转保护（原 `X_V2_Reset_Clog_Pro`）
pub fn reset_clog_pro(addr: u8) -> Command {
    Command::new([addr, 0x0E, 0x52, CHECK])
}

/// 恢复出厂设置（原 `X_V2_Restore_Motor`）
pub fn restore_motor(addr: u8) -> Command {
    Command::new([addr, 0x0F, 0x5F, CHECK])
}

/**********************************************************
*** 运动控制命令
**********************************************************/

/// 使能信号控制（原 `X_V2_En_Control`）
///
/// * `state` ：使能状态，true为使能电机，false为关闭电机
/// * `sn_f`  ：多机同步标志，false为不启用，true为启用
pub fn en_control(addr: u8, state: bool, sn_f: bool) -> Command {
    Command::new([addr, 0xF3, 0xAB, state as u8, sn_f as u8, CHECK])
}

/// 力矩模式（原 `X_V2_Torque_Control`）
///
/// * `sign`   ：符号（方向），0为正，1为负
/// * `t_ramp` ：电流斜率(Ma/s)，范围0 - 65535Ma/s
/// * `torque` ：力矩电流(Ma)，范围0 - 6000Ma
/// * `sn_f`   ：多机同步标志，false为不启用，true为启用
pub fn torque_control(addr: u8, sign: u8, t_ramp: u16, torque: u16, sn_f: bool) -> Command {
    let mut cmd = [0u8; 9];
    cmd[0] = addr;
    cmd[1] = 0xF5;
    cmd[2] = sign;
    cmd[3..5].copy_from_slice(&t_ramp.to_be_bytes());
    cmd[5..7].copy_from_slice(&torque.to_be_bytes());
    cmd[7] = sn_f as u8;
    cmd[8] = CHECK;
    Command::new(cmd)
}

/// 力矩模式限速控制（X42S/Y42）（原 `X_V2_Torque_LV_Control`）
///
/// * `sign`    ：符号（方向），0为正，1为负
/// * `t_ramp`  ：电流斜率(Ma/s)，范围0 - 65535Ma/s
/// * `torque`  ：力矩电流(Ma)，范围0 - 6000Ma
/// * `sn_f`    ：多机同步标志，false为不启用，true为启用
/// * `max_vel` ：最大速度(RPM)，范围0.0 - 3000.0RPM
pub fn torque_lv_control(
    addr: u8,
    sign: u8,
    t_ramp: u16,
    torque: u16,
    sn_f: bool,
    max_vel: f32,
) -> Command {
    // 将速度放大10倍发送过去
    let v = (max_vel * 10.0).abs() as u16;

    let mut cmd = [0u8; 11];
    cmd[0] = addr;
    cmd[1] = 0xC5;
    cmd[2] = sign;
    cmd[3..5].copy_from_slice(&t_ramp.to_be_bytes());
    cmd[5..7].copy_from_slice(&torque.to_be_bytes());
    cmd[7] = sn_f as u8;
    cmd[8..10].copy_from_slice(&v.to_be_bytes());
    cmd[10] = CHECK;
    Command::new(cmd)
}

/// 速度模式（原 `X_V2_Vel_Control`）
///
/// * `dir`   ：方向，0为CW，1为CCW
/// * `acc`   ：加速度(RPM/s)，范围0 - 65535RPM/s
/// * `vel`   ：速度(RPM)，范围0.0 - 3000.0RPM
/// * `sn_f`  ：多机同步标志，false为不启用，true为启用
pub fn vel_control(addr: u8, dir: u8, acc: u16, vel: f32, sn_f: bool) -> Command {
    // 将速度放大10倍发送过去
    let v = (vel * 10.0).abs() as u16;

    let mut cmd = [0u8; 9];
    cmd[0] = addr;
    cmd[1] = 0xF6;
    cmd[2] = dir;
    cmd[3..5].copy_from_slice(&acc.to_be_bytes());
    cmd[5..7].copy_from_slice(&v.to_be_bytes());
    cmd[7] = sn_f as u8;
    cmd[8] = CHECK;
    Command::new(cmd)
}

/// 速度模式限电流控制（X42S/Y42）（原 `X_V2_Vel_LC_Control`）
///
/// * `dir`     ：方向，0为CW，1为CCW
/// * `acc`     ：加速度(RPM/s)，范围0 - 65535RPM/s
/// * `vel`     ：速度(RPM)，范围0.0 - 3000.0RPM
/// * `sn_f`    ：多机同步标志，false为不启用，true为启用
/// * `max_cur` ：最大电流(mA)，范围0 - 6000mA
pub fn vel_lc_control(
    addr: u8,
    dir: u8,
    acc: u16,
    vel: f32,
    sn_f: bool,
    max_cur: u16,
) -> Command {
    // 将速度放大10倍发送过去
    let v = (vel * 10.0).abs() as u16;

    let mut cmd = [0u8; 11];
    cmd[0] = addr;
    cmd[1] = 0xC6;
    cmd[2] = dir;
    cmd[3..5].copy_from_slice(&acc.to_be_bytes());
    cmd[5..7].copy_from_slice(&v.to_be_bytes());
    cmd[7] = sn_f as u8;
    cmd[8..10].copy_from_slice(&max_cur.to_be_bytes());
    cmd[10] = CHECK;
    Command::new(cmd)
}

/// 直通限速位置模式（原 `X_V2_Bypass_Pos_LV_Control`）
///
/// * `dir`   ：方向，0为CW，1为CCW
/// * `vel`   ：运动速度(RPM)，范围0.0 - 3000.0RPM
/// * `pos`   ：位置角度(°)，范围0.0°- (2^32 - 1) / 10°
/// * `raf`   ：相位/绝对运动标志，0为相对上一次输入目标位置，1为绝对位置，2为相对当前实时位置
/// * `sn_f`  ：多机同步标志，false为不启用，true为启用
pub fn bypass_pos_lv_control(
    addr: u8,
    dir: u8,
    vel: f32,
    pos: f32,
    raf: u8,
    sn_f: bool,
) -> Command {
    // 将速度和位置放大10倍发送过去
    let v = (vel * 10.0).abs() as u16;
    let p = (pos * 10.0).abs() as u32;

    let mut cmd = [0u8; 12];
    cmd[0] = addr;
    cmd[1] = 0xFB;
    cmd[2] = dir;
    cmd[3..5].copy_from_slice(&v.to_be_bytes());
    cmd[5..9].copy_from_slice(&p.to_be_bytes());
    cmd[9] = raf;
    cmd[10] = sn_f as u8;
    cmd[11] = CHECK;
    Command::new(cmd)
}

/// 直通限速位置模式限电流控制（原 `X_V2_Bypass_Pos_LV_LC_Control`）
///
/// * `dir`     ：方向，0为CW，1为CCW
/// * `vel`     ：运动速度(RPM)，范围0.0 - 3000.0RPM
/// * `pos`     ：位置角度(°)，范围0.0°- (2^32 - 1) / 10°
/// * `raf`     ：相位/绝对运动标志，0为相对上一次输入目标位置，1为绝对位置，2为相对当前实时位置
/// * `sn_f`    ：多机同步标志，false为不启用，true为启用
/// * `max_cur` ：最大电流(mA)，范围0 - 6000mA
#[allow(clippy::too_many_arguments)]
pub fn bypass_pos_lv_lc_control(
    addr: u8,
    dir: u8,
    vel: f32,
    pos: f32,
    raf: u8,
    sn_f: bool,
    max_cur: u16,
) -> Command {
    // 将速度和位置放大10倍发送过去
    let v = (vel * 10.0).abs() as u16;
    let p = (pos * 10.0).abs() as u32;

    let mut cmd = [0u8; 14];
    cmd[0] = addr;
    cmd[1] = 0xCB;
    cmd[2] = dir;
    cmd[3..5].copy_from_slice(&v.to_be_bytes());
    cmd[5..9].copy_from_slice(&p.to_be_bytes());
    cmd[9] = raf;
    cmd[10] = sn_f as u8;
    cmd[11..13].copy_from_slice(&max_cur.to_be_bytes());
    cmd[13] = CHECK;
    Command::new(cmd)
}

/// 梯形曲线加减速位置模式控制（原 `X_V2_Traj_Pos_Control`）
///
/// * `dir`   ：方向，0为CW，其余值为CCW
/// * `acc`   ：加速加速度(RPM/s)
/// * `dec`   ：减速加速度(RPM/s)
/// * `vel`   ：最大速度(RPM)，范围0.0 - 3000.0RPM
/// * `pos`   ：位置(°)，范围0.0°- (2^32 - 1)°
/// * `raf`   ：相位/绝对运动标志，0为相对上一次输入目标位置，1为绝对位置，2为相对当前实时位置
/// * `sn_f`  ：多机同步标志，false为不启用，true为启用
#[allow(clippy::too_many_arguments)]
pub fn traj_pos_control(
    addr: u8,
    dir: u8,
    acc: u16,
    dec: u16,
    vel: f32,
    pos: f32,
    raf: u8,
    sn_f: bool,
) -> Command {
    // 将速度和位置放大10倍发送过去
    let v = (vel * 10.0).abs() as u16;
    let p = (pos * 10.0).abs() as u32;

    let mut cmd = [0u8; 16];
    cmd[0] = addr;
    cmd[1] = 0xFD;
    cmd[2] = dir;
    cmd[3..5].copy_from_slice(&acc.to_be_bytes());
    cmd[5..7].copy_from_slice(&dec.to_be_bytes());
    cmd[7..9].copy_from_slice(&v.to_be_bytes());
    cmd[9..13].copy_from_slice(&p.to_be_bytes());
    cmd[13] = raf;
    cmd[14] = sn_f as u8;
    cmd[15] = CHECK;
    Command::new(cmd)
}

/// 梯形曲线加减速位置模式限电流控制（X42S/Y42）（原 `X_V2_Traj_Pos_LC_Control`）
///
/// * `dir`     ：方向，0为CW，其余值为CCW
/// * `acc`     ：加速加速度(RPM/s)
/// * `dec`     ：减速加速度(RPM/s)
/// * `vel`     ：最大速度(RPM)，范围0.0 - 3000.0RPM
/// * `pos`     ：位置(°)，范围0.0°- (2^32 - 1)°
/// * `raf`     ：相位/绝对运动标志，0为相对上一次输入目标位置，1为绝对位置，2为相对当前实时位置
/// * `sn_f`    ：多机同步标志，false为不启用，true为启用
/// * `max_cur` ：最大电流(mA)，范围0 - 6000mA
#[allow(clippy::too_many_arguments)]
pub fn traj_pos_lc_control(
    addr: u8,
    dir: u8,
    acc: u16,
    dec: u16,
    vel: f32,
    pos: f32,
    raf: u8,
    sn_f: bool,
    max_cur: u16,
) -> Command {
    // 将速度和位置放大10倍发送过去
    let v = (vel * 10.0).abs() as u16;
    let p = (pos * 10.0).abs() as u32;

    let mut cmd = [0u8; 18];
    cmd[0] = addr;
    cmd[1] = 0xCD;
    cmd[2] = dir;
    cmd[3..5].copy_from_slice(&acc.to_be_bytes());
    cmd[5..7].copy_from_slice(&dec.to_be_bytes());
    cmd[7..9].copy_from_slice(&v.to_be_bytes());
    cmd[9..13].copy_from_slice(&p.to_be_bytes());
    cmd[13] = raf;
    cmd[14] = sn_f as u8;
    cmd[15..17].copy_from_slice(&max_cur.to_be_bytes());
    cmd[17] = CHECK;
    Command::new(cmd)
}

/// 设置快速梯形曲线位置模式的运动参数（X42S/Y42）（原 `X_V2_Set_QTrajPos_Params`）
///
/// * `acc`     ：加速加速度(RPM/s)
/// * `dec`     ：减速加速度(RPM/s)
/// * `vel`     ：最大速度(RPM)，范围0.0 - 3000.0RPM
/// * `raf`     ：相位/绝对运动标志，0为相对上一次输入目标位置，1为绝对位置，2为相对电机当前实时位置
/// * `sn_f`    ：多机同步标志，false为不启用，true为启用
/// * `max_cur` ：最大电流(mA)，范围0 - 6000mA
pub fn set_qtraj_pos_params(
    addr: u8,
    acc: u16,
    dec: u16,
    vel: f32,
    raf: u8,
    sn_f: bool,
    max_cur: u16,
) -> Command {
    // 将速度放大10倍发送过去
    let v = (vel * 10.0).abs() as u16;

    let mut cmd = [0u8; 13];
    cmd[0] = addr;
    cmd[1] = 0xF1;
    cmd[2..4].copy_from_slice(&acc.to_be_bytes());
    cmd[4..6].copy_from_slice(&dec.to_be_bytes());
    cmd[6..8].copy_from_slice(&v.to_be_bytes());
    cmd[8] = raf;
    cmd[9] = sn_f as u8;
    cmd[10..12].copy_from_slice(&max_cur.to_be_bytes());
    cmd[12] = CHECK;
    Command::new(cmd)
}

/// 快速梯形曲线位置模式控制（X42S/Y42）（原 `X_V2_QTrajPos_LC_Control`）
///
/// * `pos` ：位置角度(带符号)，单位：0.1°
pub fn qtraj_pos_lc_control(addr: u8, pos: f32) -> Command {
    // 将位置放大10倍发送过去（保留一位小数）
    let p = (pos * 10.0).abs() as u32;

    let mut cmd = [0u8; 7];
    cmd[0] = addr;
    cmd[1] = 0xFC;
    cmd[2..6].copy_from_slice(&p.to_be_bytes());
    cmd[6] = CHECK;
    Command::new(cmd)
}

/// 立即停止（原 `X_V2_Stop_Now`）
///
/// * `sn_f` ：多机同步标志，false为不启用，true为启用
pub fn stop_now(addr: u8, sn_f: bool) -> Command {
    Command::new([addr, 0xFE, 0x98, sn_f as u8, CHECK])
}

/// 多机同步运动（原 `X_V2_Synchronous_motion`）
pub fn synchronous_motion(addr: u8) -> Command {
    Command::new([addr, 0xFF, 0x66, CHECK])
}

/**********************************************************
*** 原点回零命令
**********************************************************/

/// 设置单圈回零的零点位置（原 `X_V2_Origin_Set_O`）
///
/// * `sv_f` ：是否存储标志，false为不存储，true为存储
pub fn origin_set_o(addr: u8, sv_f: bool) -> Command {
    Command::new([addr, 0x93, 0x88, sv_f as u8, CHECK])
}

/// 触发回零（原 `X_V2_Origin_Trigger_Return`）
///
/// * `o_mode` ：回零模式，0为单圈就近回零，1为单圈方向回零，2为多圈无限位碰撞回零，3为多圈有限位开关回零
/// * `sn_f`   ：多机同步标志，false为不启用，true为启用
pub fn origin_trigger_return(addr: u8, o_mode: u8, sn_f: bool) -> Command {
    Command::new([addr, 0x9A, o_mode, sn_f as u8, CHECK])
}

/// 强制中断并退出回零（原 `X_V2_Origin_Interrupt`）
pub fn origin_interrupt(addr: u8) -> Command {
    Command::new([addr, 0x9C, 0x48, CHECK])
}

/// 读取回零参数（原 `X_V2_Origin_Read_Params`）
pub fn origin_read_params(addr: u8) -> Command {
    Command::new([addr, 0x22, CHECK])
}

/// 修改回零参数（原 `X_V2_Origin_Modify_Params`）
///
/// * `sv_f`   ：是否存储标志，false为不存储，true为存储
/// * `o_mode` ：回零模式，0为单圈就近回零，1为单圈方向回零，2为多圈无限位碰撞回零，3为多圈有限位开关回零
/// * `o_dir`  ：回零方向，0为CW，其余值为CCW
/// * `o_vel`  ：回零速度，单位：RPM（转/分钟）
/// * `o_tm`   ：回零超时时间，单位：毫秒
/// * `sl_vel` ：无限位碰撞回零检测转速，单位：RPM（转/分钟）
/// * `sl_ma`  ：无限位碰撞回零检测电流，单位：Ma（毫安）
/// * `sl_ms`  ：无限位碰撞回零检测时间，单位：Ms（毫秒）
/// * `pot_f`  ：上电自动触发回零，false为不使能，true为使能
#[allow(clippy::too_many_arguments)]
pub fn origin_modify_params(
    addr: u8,
    sv_f: bool,
    o_mode: u8,
    o_dir: u8,
    o_vel: u16,
    o_tm: u32,
    sl_vel: u16,
    sl_ma: u16,
    sl_ms: u16,
    pot_f: bool,
) -> Command {
    let mut cmd = [0u8; 20];
    cmd[0] = addr;
    cmd[1] = 0x4C;
    cmd[2] = 0xAE;
    cmd[3] = sv_f as u8;
    cmd[4] = o_mode;
    cmd[5] = o_dir;
    cmd[6..8].copy_from_slice(&o_vel.to_be_bytes());
    cmd[8..12].copy_from_slice(&o_tm.to_be_bytes());
    cmd[12..14].copy_from_slice(&sl_vel.to_be_bytes());
    cmd[14..16].copy_from_slice(&sl_ma.to_be_bytes());
    cmd[16..18].copy_from_slice(&sl_ms.to_be_bytes());
    cmd[18] = pot_f as u8;
    cmd[19] = CHECK;
    Command::new(cmd)
}

/// 读取碰撞回零返回角度（X42S/Y42）（原 `X_V2_Origin_Read_SL_RP`）
pub fn origin_read_sl_rp(addr: u8) -> Command {
    Command::new([addr, 0x3F, CHECK])
}

/// 修改碰撞回零返回角度（X42S/Y42）（原 `X_V2_Origin_Modify_SL_RP`）
///
/// * `sv_f`  ：是否存储标志，false为不存储，true为存储
/// * `sl_rp` ：碰撞回零返回角度，单位0.1°，即给40，就是4.0°
pub fn origin_modify_sl_rp(addr: u8, sv_f: bool, sl_rp: u16) -> Command {
    let mut cmd = [0u8; 7];
    cmd[0] = addr;
    cmd[1] = 0x5C;
    cmd[2] = 0xAC;
    cmd[3] = sv_f as u8;
    cmd[4..6].copy_from_slice(&sl_rp.to_be_bytes());
    cmd[6] = CHECK;
    Command::new(cmd)
}

/**********************************************************
*** 读取系统参数命令
**********************************************************/

/// 定时返回信息命令（X42S/Y42）（原 `X_V2_Auto_Return_Sys_Params_Timed`）
///
/// * `s`       ：系统参数类型
/// * `time_ms` ：定时时间
pub fn auto_return_sys_params_timed(addr: u8, s: SysParams, time_ms: u16) -> Command {
    let mut buf = [0u8; MAX_CMD_LEN];
    let mut len = 0;

    buf[len] = addr;
    len += 1;
    buf[len] = 0x11;
    len += 1;
    buf[len] = 0x18;
    len += 1;

    // 信息功能码（该命令不支持 Sys，与 C 版本行为一致：不发信息码字节）
    if let Some(code) = s.timed_code() {
        buf[len] = code;
        len += 1;
    }

    buf[len..len + 2].copy_from_slice(&time_ms.to_be_bytes());
    len += 2;

    buf[len] = CHECK;
    len += 1;

    Command::from_parts(buf, len)
}

/// 读取系统参数（原 `X_V2_Read_Sys_Params`）
///
/// * `s` ：系统参数类型
pub fn read_sys_params(addr: u8, s: SysParams) -> Command {
    let mut buf = [0u8; MAX_CMD_LEN];
    let mut len = 0;

    buf[len] = addr;
    len += 1;

    // 功能码（Sys 需要多补一个辅助码 0x7A）
    let (code, aux) = s.code();
    buf[len] = code;
    len += 1;
    if let Some(aux) = aux {
        buf[len] = aux;
        len += 1;
    }

    buf[len] = CHECK;
    len += 1;

    Command::from_parts(buf, len)
}

/**********************************************************
*** 读写驱动参数命令
**********************************************************/

/// 修改电机ID地址（原 `X_V2_Modify_Motor_ID`）
///
/// * `sv_f` ：是否存储标志，false为不存储，true为存储
/// * `id`   ：默认电机ID为1，可修改为1-255，0为广播地址
pub fn modify_motor_id(addr: u8, sv_f: bool, id: u8) -> Command {
    Command::new([addr, 0xAE, 0x4B, sv_f as u8, id, CHECK])
}

/// 修改细分值（原 `X_V2_Modify_MicroStep`）
///
/// * `sv_f`  ：是否存储标志，false为不存储，true为存储
/// * `mstep` ：默认细分为16，可修改为1-2556，0为256细分
pub fn modify_micro_step(addr: u8, sv_f: bool, mstep: u8) -> Command {
    Command::new([addr, 0x84, 0x8A, sv_f as u8, mstep, CHECK])
}

/// 修改掉电标志（原 `X_V2_Modify_PDFlag`）
///
/// * `pdf` ：掉电标志
pub fn modify_pd_flag(addr: u8, pdf: bool) -> Command {
    Command::new([addr, 0x50, pdf as u8, CHECK])
}

/// 读取选项参数状态（X42S/Y42）（原 `X_V2_Read_Opt_Param_Sta`）
pub fn read_opt_param_sta(addr: u8) -> Command {
    Command::new([addr, 0x1A, CHECK])
}

/// 修改电机类型（X42S/Y42）（原 `X_V2_Modify_Motor_Type`）
///
/// * `sv_f`    ：是否存储标志，false为不存储，true为存储
/// * `mottype` ：电机类型，默认为0，0表示1.8°步进电机，1表示0.9°步进电机
pub fn modify_motor_type(addr: u8, sv_f: bool, mottype: bool) -> Command {
    // 电机类型，0表示0.9°步进电机，1表示1.8°步进电机
    let mot_type = if mottype { 25 } else { 50 };
    Command::new([addr, 0xD7, 0x35, sv_f as u8, mot_type, CHECK])
}

/// 修改固件类型（X42S/Y42）（原 `X_V2_Modify_Firmware_Type`）
///
/// * `sv_f`   ：是否存储标志，false为不存储，true为存储
/// * `fwtype` ：固件类型，默认为0，0为X固件，1为Emm固件
pub fn modify_firmware_type(addr: u8, sv_f: bool, fwtype: bool) -> Command {
    Command::new([addr, 0xD5, 0x69, sv_f as u8, fwtype as u8, CHECK])
}

/// 修改开环/闭环控制模式（X42S/Y42）（原 `X_V2_Modify_Ctrl_Mode`）
///
/// * `sv_f`      ：是否存储标志，false为不存储，true为存储
/// * `ctrl_mode` ：控制模式，默认为1,0为开环模式，1为闭环FOC模式
pub fn modify_ctrl_mode(addr: u8, sv_f: bool, ctrl_mode: bool) -> Command {
    Command::new([addr, 0x46, 0x69, sv_f as u8, ctrl_mode as u8, CHECK])
}

/// 修改电机运动正方向（X42S/Y42）（原 `X_V2_Modify_Motor_Dir`）
///
/// * `sv_f` ：是否存储标志，false为不存储，true为存储
/// * `dir`  ：电机运动正方向，默认为CW，0为CW（顺时针方向），1为CCW
pub fn modify_motor_dir(addr: u8, sv_f: bool, dir: bool) -> Command {
    Command::new([addr, 0xD4, 0x60, sv_f as u8, dir as u8, CHECK])
}

/// 修改锁定按键功能（X42S/Y42）（原 `X_V2_Modify_Lock_Btn`）
///
/// * `sv_f` ：是否存储标志，false为不存储，true为存储
/// * `lock` ：锁定按键功能，默认为Disable，0为Disable，1为Enable
pub fn modify_lock_btn(addr: u8, sv_f: bool, lock: bool) -> Command {
    Command::new([addr, 0xD0, 0xB3, sv_f as u8, lock as u8, CHECK])
}

/// 修改命令位置角度是否继续缩小10倍输入（X42S/Y42）（原 `X_V2_Modify_S_Vel`）
///
/// * `sv_f`  ：是否存储标志，false为不存储，true为存储
/// * `s_vel` ：命令位置角度是否继续缩小10倍输入，默认为Disable，0为Disable，1为Enable
pub fn modify_s_vel(addr: u8, sv_f: bool, s_vel: bool) -> Command {
    Command::new([addr, 0x4F, 0x71, sv_f as u8, s_vel as u8, CHECK])
}

/// 修改开环模式工作电流（原 `X_V2_Modify_OM_mA`）
///
/// * `sv_f`  ：是否存储标志，false为不存储，true为存储
/// * `om_ma` ：开环模式工作电流，单位mA
pub fn modify_om_ma(addr: u8, sv_f: bool, om_ma: u16) -> Command {
    let mut cmd = [0u8; 7];
    cmd[0] = addr;
    cmd[1] = 0x44;
    cmd[2] = 0x33;
    cmd[3] = sv_f as u8;
    cmd[4..6].copy_from_slice(&om_ma.to_be_bytes());
    cmd[6] = CHECK;
    Command::new(cmd)
}

/// 修改闭环模式最大电流（原 `X_V2_Modify_FOC_mA`）
///
/// * `sv_f`   ：是否存储标志，false为不存储，true为存储
/// * `foc_ma` ：闭环模式最大电流，单位mA
pub fn modify_foc_ma(addr: u8, sv_f: bool, foc_ma: u16) -> Command {
    let mut cmd = [0u8; 7];
    cmd[0] = addr;
    cmd[1] = 0x45;
    cmd[2] = 0x66;
    cmd[3] = sv_f as u8;
    cmd[4..6].copy_from_slice(&foc_ma.to_be_bytes());
    cmd[6] = CHECK;
    Command::new(cmd)
}

/// 读取PID参数（原 `X_V2_Read_PID_Params`）
pub fn read_pid_params(addr: u8) -> Command {
    Command::new([addr, 0x21, CHECK])
}

/// 修改PID参数（原 `X_V2_Modify_PID_Params`）
///
/// * `sv_f`  ：是否存储标志，false为不存储，true为存储
/// * `p_tkp` ：梯形曲线位置环比例系数，默认为126640
/// * `p_bkp` ：直通限速位置环比例系数，默认为126640
/// * `vkp`   ：速度环比例系数，42默认为15600
/// * `vki`   ：速度环积分系数，42默认为26
pub fn modify_pid_params(
    addr: u8,
    sv_f: bool,
    p_tkp: u32,
    p_bkp: u32,
    vkp: u32,
    vki: u32,
) -> Command {
    let mut cmd = [0u8; 21];
    cmd[0] = addr;
    cmd[1] = 0x4A;
    cmd[2] = 0xC3;
    cmd[3] = sv_f as u8;
    cmd[4..8].copy_from_slice(&p_tkp.to_be_bytes());
    cmd[8..12].copy_from_slice(&p_bkp.to_be_bytes());
    cmd[12..16].copy_from_slice(&vkp.to_be_bytes());
    cmd[16..20].copy_from_slice(&vki.to_be_bytes());
    cmd[20] = CHECK;
    Command::new(cmd)
}

/// 读取DMX512协议参数（X42S/Y42）（原 `X_V2_Read_DMX512_Params`）
pub fn read_dmx512_params(addr: u8) -> Command {
    Command::new([addr, 0x49, 0x78, CHECK])
}

/// 修改DMX512协议参数（X42S/Y42）（原 `X_V2_Modify_DMX512_Params`）
///
/// * `sv_f`     ：是否存储标志，false为不存储，true为存储
/// * `tch`      ：总通道数，默认为192，该值要与自身 DMX512 控制器的总通道数一样
/// * `nch`      ：每个电机占用的通道数，默认为1，1为单通道模式,2为双通道模式
/// * `mode`     ：运动模式，默认为1，0表示相对位置模式运动，1表示绝对坐标式位置运动
/// * `vel`      ：单通道模式的运动速度，默认值为1000， 单位RPM， 即1000RPM；
/// * `acc`      ：加速度，acc=加速数值/8=125，加速时间见说明书“5.3.12 位置模式控制（Emm）”
/// * `vel_step` ：双通道模式速度步长，默认值为 10， 即电机运动速度为(通道值 * 10)RPM
/// * `pos_step` ：双通道模式运动步长，默认值为 100， 即电机转动角度为(通道值 * 10.0)°
#[allow(clippy::too_many_arguments)]
pub fn modify_dmx512_params(
    addr: u8,
    sv_f: bool,
    tch: u16,
    nch: u8,
    mode: u8,
    vel: u16,
    acc: u16,
    vel_step: u16,
    pos_step: u32,
) -> Command {
    let mut cmd = [0u8; 19];
    cmd[0] = addr;
    cmd[1] = 0xD9;
    cmd[2] = 0x90;
    cmd[3] = sv_f as u8;
    cmd[4..6].copy_from_slice(&tch.to_be_bytes());
    cmd[6] = nch;
    cmd[7] = mode;
    cmd[8..10].copy_from_slice(&vel.to_be_bytes());
    cmd[10..12].copy_from_slice(&acc.to_be_bytes());
    cmd[12..14].copy_from_slice(&vel_step.to_be_bytes());
    cmd[14..18].copy_from_slice(&pos_step.to_be_bytes());
    cmd[18] = CHECK;
    Command::new(cmd)
}

/// 读取位置到达窗口（X42S/Y42）（原 `X_V2_Read_Pos_Window`）
pub fn read_pos_window(addr: u8) -> Command {
    Command::new([addr, 0x41, CHECK])
}

/// 修改位置到达窗口（X42S/Y42）（原 `X_V2_Modify_Pos_Window`）
///
/// * `sv_f` ：是否存储标志，false为不存储，true为存储
/// * `prw`  ：位置到达窗口，默认值为8，即0.8°
pub fn modify_pos_window(addr: u8, sv_f: bool, prw: u16) -> Command {
    let mut cmd = [0u8; 7];
    cmd[0] = addr;
    cmd[1] = 0xD1;
    cmd[2] = 0x07;
    cmd[3] = sv_f as u8;
    cmd[4..6].copy_from_slice(&prw.to_be_bytes());
    cmd[6] = CHECK;
    Command::new(cmd)
}

/// 读取过热过流保护检测阈值（X42S/Y42）（原 `X_V2_Read_Otocp`）
pub fn read_otocp(addr: u8) -> Command {
    Command::new([addr, 0x13, CHECK])
}

/// 修改过热过流保护检测阈值（X42S/Y42）（原 `X_V2_Modify_Otocp`）
///
/// * `sv_f`    ：是否存储标志，false为不存储，true为存储
/// * `otp`     ：过热保护检测阈值，默认100℃
/// * `ocp`     ：过流保护检测阈值，默认6600mA
/// * `time_ms` ：过热过流检测时间，默认1000ms
pub fn modify_otocp(addr: u8, sv_f: bool, otp: u16, ocp: u16, time_ms: u16) -> Command {
    let mut cmd = [0u8; 11];
    cmd[0] = addr;
    cmd[1] = 0xD3;
    cmd[2] = 0x56;
    cmd[3] = sv_f as u8;
    cmd[4..6].copy_from_slice(&otp.to_be_bytes());
    cmd[6..8].copy_from_slice(&ocp.to_be_bytes());
    cmd[8..10].copy_from_slice(&time_ms.to_be_bytes());
    cmd[10] = CHECK;
    Command::new(cmd)
}

/// 读取心跳保护功能时间（X42S/Y42）（原 `X_V2_Read_Heart_Protect`）
pub fn read_heart_protect(addr: u8) -> Command {
    Command::new([addr, 0x16, CHECK])
}

/// 修改心跳保护功能时间（X42S/Y42）（原 `X_V2_Modify_Heart_Protect`）
///
/// * `sv_f` ：是否存储标志，false为不存储，true为存储
/// * `hp`   ：心跳保护时间，单位：ms
pub fn modify_heart_protect(addr: u8, sv_f: bool, hp: u32) -> Command {
    let mut cmd = [0u8; 9];
    cmd[0] = addr;
    cmd[1] = 0x68;
    cmd[2] = 0x38;
    cmd[3] = sv_f as u8;
    cmd[4..8].copy_from_slice(&hp.to_be_bytes());
    cmd[8] = CHECK;
    Command::new(cmd)
}

/// 读取积分限幅/刚性系数（X42S/Y42）（原 `X_V2_Read_Integral_Limit`）
pub fn read_integral_limit(addr: u8) -> Command {
    Command::new([addr, 0x23, CHECK])
}

/// 修改积分限幅/刚性系数（X42S/Y42）（原 `X_V2_Modify_Integral_Limit`）
///
/// * `sv_f` ：是否存储标志，false为不存储，true为存储
/// * `il`   ：刚性系数，X 固件默认为X42S/Y42/388、X57S/Y57/512
pub fn modify_integral_limit(addr: u8, sv_f: bool, il: u32) -> Command {
    let mut cmd = [0u8; 9];
    cmd[0] = addr;
    cmd[1] = 0x4B;
    cmd[2] = 0x57;
    cmd[3] = sv_f as u8;
    cmd[4..8].copy_from_slice(&il.to_be_bytes());
    cmd[8] = CHECK;
    Command::new(cmd)
}

/**********************************************************
*** 读取所有驱动参数命令
**********************************************************/

/// 读取系统状态参数（原 `X_V2_Read_System_State_Params`）
pub fn read_system_state_params(addr: u8) -> Command {
    Command::new([addr, 0x43, 0x7A, CHECK])
}

/// 读取驱动配置参数（原 `X_V2_Read_Motor_Conf_Params`）
pub fn read_motor_conf_params(addr: u8) -> Command {
    Command::new([addr, 0x42, 0x6C, CHECK])
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 记录发送帧的 mock CAN 总线
    struct MockCan {
        /// 已发送的帧：(扩展帧ID, 数据, 数据长度)
        frames: [(u32, [u8; 8], usize); 16],
        n: usize,
        /// 待接收的帧队列（测试用例预置）
        rx: [(u32, [u8; 8], usize); 16],
        rn: usize,
        rx_head: usize,
    }

    impl Default for MockCan {
        fn default() -> Self {
            Self {
                frames: [(0, [0; 8], 0); 16],
                n: 0,
                rx: [(0, [0; 8], 0); 16],
                rn: 0,
                rx_head: 0,
            }
        }
    }

    impl MockCan {
        /// 第 i 帧的 (扩展帧ID, 数据)
        fn frame(&self, i: usize) -> (u32, &[u8]) {
            let (id, data, len) = &self.frames[i];
            (*id, &data[..*len])
        }

        /// 预置一帧待接收数据
        fn push_rx(&mut self, ext_id: u32, data: &[u8]) {
            let len = data.len();
            self.rx[self.rn].0 = ext_id;
            self.rx[self.rn].1[..len].copy_from_slice(data);
            self.rx[self.rn].2 = len;
            self.rn += 1;
        }
    }

    impl CanTx for MockCan {
        fn send_frame(&mut self, ext_id: u32, data: &[u8]) {
            let len = data.len();
            self.frames[self.n].0 = ext_id;
            self.frames[self.n].1[..len].copy_from_slice(data);
            self.frames[self.n].2 = len;
            self.n += 1;
        }
    }

    impl CanRx for MockCan {
        fn try_receive_frame(&mut self) -> Option<(u32, &[u8])> {
            if self.rx_head >= self.rn {
                return None;
            }
            let idx = self.rx_head;
            self.rx_head += 1;
            let (id, data, len) = &self.rx[idx];
            Some((*id, &data[..*len]))
        }
    }

    #[test]
    fn build_trig_encoder_cal() {
        assert_eq!(trig_encoder_cal(0x01).as_bytes(), &[0x01, 0x06, 0x45, 0x6B]);
    }

    #[test]
    fn build_en_control() {
        assert_eq!(
            en_control(0x01, true, false).as_bytes(),
            &[0x01, 0xF3, 0xAB, 0x01, 0x00, 0x6B]
        );
    }

    #[test]
    fn build_vel_control() {
        // vel = 120.5 RPM，放大10倍 -> 1205 = 0x04B5
        assert_eq!(
            vel_control(0x01, 0, 100, 120.5, false).as_bytes(),
            &[0x01, 0xF6, 0x00, 0x00, 0x64, 0x04, 0xB5, 0x00, 0x6B]
        );
    }

    #[test]
    fn build_bypass_pos_lv_lc_control() {
        // vel = 100.0 -> 1000 = 0x03E8；pos = 36.0 -> 360 = 0x00000168；max_cur = 2000 = 0x07D0
        assert_eq!(
            bypass_pos_lv_lc_control(0x01, 0, 100.0, 36.0, 0, false, 2000).as_bytes(),
            &[
                0x01, 0xCB, 0x00, 0x03, 0xE8, 0x00, 0x00, 0x01, 0x68, 0x00, 0x00, 0x07, 0xD0,
                0x6B
            ]
        );
    }

    #[test]
    fn build_read_sys_params() {
        assert_eq!(
            read_sys_params(0x01, SysParams::Vel).as_bytes(),
            &[0x01, 0x35, 0x6B]
        );
        // Sys 需要多补一个辅助码 0x7A
        assert_eq!(
            read_sys_params(0x01, SysParams::Sys).as_bytes(),
            &[0x01, 0x43, 0x7A, 0x6B]
        );
    }

    #[test]
    fn send_cmd_splits_long_command() {
        let mut can = MockCan::default();
        // 数据 12 字节 > 7，拆两包发送
        CanTx::send_cmd(&mut can, bypass_pos_lv_lc_control(0x01, 0, 100.0, 36.0, 0, false, 2000).as_bytes());
        assert_eq!(can.n, 2);
        assert_eq!(
            can.frame(0),
            (0x0100, &[0xCB, 0x00, 0x03, 0xE8, 0x00, 0x00, 0x01, 0x68][..])
        );
        assert_eq!(
            can.frame(1),
            (0x0101, &[0xCB, 0x00, 0x00, 0x07, 0xD0, 0x6B][..])
        );
    }

    #[test]
    fn transaction_commit() {
        let mut t = Transaction::begin();
        t.load(trig_encoder_cal(0x01)).load(stop_now(0x02, false));
        let batch = t.commit(0x00).unwrap();
        assert_eq!(
            batch.as_bytes(),
            &[
                0x00, 0xAA, 0x00, 0x0E, // 地址 + 功能码 + 总字节数(9 + 5 = 14)
                0x01, 0x06, 0x45, 0x6B, // 子命令1：触发编码器校准
                0x02, 0xFE, 0x98, 0x00, 0x6B, // 子命令2：立即停止
                0x6B,
            ]
        );
    }

    #[test]
    fn transaction_empty_commit_is_none() {
        assert!(Transaction::begin().commit(0x00).is_none());
    }

    #[test]
    fn write_read_filters_by_addr() {
        let mut can = MockCan::default();
        // 先到来一帧其他电机的帧（应被丢弃），再来目标电机的应答
        can.push_rx(0x0200, &[0x35, 0x00, 0x64]); // addr = 2
        can.push_rx(0x0100, &[0x35, 0x04, 0xB5]); // addr = 1，目标应答

        let cmd = read_sys_params(0x01, SysParams::Vel);
        let resp = CanBus::write_read(&mut can, cmd.as_bytes(), 16).unwrap();
        assert_eq!(resp.ext_id, 0x0100);
        assert_eq!(&resp.data[..resp.len], &[0x35, 0x04, 0xB5]);
        // 命令帧也确实发出去了
        assert_eq!(can.frame(0), (0x0100, &[0x35, 0x6B][..]));
    }

    #[test]
    fn write_read_timeout() {
        let mut can = MockCan::default();
        // 队列里只有无关帧，轮询耗尽后应返回 None
        can.push_rx(0x0200, &[0x35, 0x00]);

        let cmd = read_sys_params(0x01, SysParams::Vel);
        assert_eq!(CanBus::write_read(&mut can, cmd.as_bytes(), 8), None);
    }

    /**********************************************************
    *** async 接口测试
    **********************************************************/

    /// 极简 block_on：本测试里的 future 都会立即就绪，空 waker 即可
    fn block_on<F: core::future::Future>(future: F) -> F::Output {
        use core::pin::pin;
        use core::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};

        fn no_op(_: *const ()) {}
        fn clone(_: *const ()) -> RawWaker {
            RawWaker::new(core::ptr::null(), &VTABLE)
        }
        static VTABLE: RawWakerVTable = RawWakerVTable::new(clone, no_op, no_op, no_op);

        let waker = unsafe { Waker::from_raw(RawWaker::new(core::ptr::null(), &VTABLE)) };
        let mut cx = Context::from_waker(&waker);
        let mut future = pin!(future);
        loop {
            match future.as_mut().poll(&mut cx) {
                Poll::Ready(v) => return v,
                Poll::Pending => {}
            }
        }
    }

    impl AsyncCanTx for MockCan {
        async fn send_frame(&mut self, ext_id: u32, data: &[u8]) {
            CanTx::send_frame(self, ext_id, data);
        }
    }

    impl AsyncCanRx for MockCan {
        async fn receive_frame(&mut self) -> CanFrame {
            let (ext_id, data) = CanRx::try_receive_frame(self).expect("测试应预置接收帧");
            let len = data.len();
            let mut frame_data = [0u8; 8];
            frame_data[..len].copy_from_slice(data);
            CanFrame {
                ext_id,
                data: frame_data,
                len,
            }
        }
    }

    #[test]
    fn async_send_cmd_splits_long_command() {
        let mut can = MockCan::default();
        let cmd = bypass_pos_lv_lc_control(0x01, 0, 100.0, 36.0, 0, false, 2000);
        block_on(AsyncCanTx::send_cmd(&mut can, cmd.as_bytes()));
        assert_eq!(can.n, 2);
        assert_eq!(
            can.frame(0),
            (0x0100, &[0xCB, 0x00, 0x03, 0xE8, 0x00, 0x00, 0x01, 0x68][..])
        );
        assert_eq!(
            can.frame(1),
            (0x0101, &[0xCB, 0x00, 0x00, 0x07, 0xD0, 0x6B][..])
        );
    }

    #[test]
    fn async_write_read_filters_by_addr() {
        let mut can = MockCan::default();
        // 先到来一帧其他电机的帧（应被丢弃），再来目标电机的应答
        can.push_rx(0x0200, &[0x35, 0x00, 0x64]); // addr = 2
        can.push_rx(0x0100, &[0x35, 0x04, 0xB5]); // addr = 1，目标应答

        let cmd = read_sys_params(0x01, SysParams::Vel);
        let resp = block_on(AsyncCanBus::write_read(&mut can, cmd.as_bytes())).unwrap();
        assert_eq!(resp.ext_id, 0x0100);
        assert_eq!(&resp.data[..resp.len], &[0x35, 0x04, 0xB5]);
        // 命令帧也确实发出去了
        assert_eq!(can.frame(0), (0x0100, &[0x35, 0x6B][..]));
    }
}



fn main() {
    // todo : try to use tokio_serial_can to send step-motor-ctrl command
    todo!("TODO")
}
