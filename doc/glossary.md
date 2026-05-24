# 游戏开发术语词典

> **目标读者**：自己（有 web / 后端 / 编译器背景，没做过游戏）。
>
> **收录原则**：只收**业界通用术语**——以后搜 GDC talk / r/gamedev / Unity 文档时会再遇到的英文词。
> 项目自己起的命名（`Strike` / `HurtRadius` / `DamagePipeline` ……）不收，rustdoc 里看就行。
>
> 每条格式：**英文**（中文译名 / 缩写全写）— 一句话定义 + 一句话补充（为什么有 / 项目里在哪用）。

---

## 1. 动作 / 攻击节奏

格斗游戏 / 动作游戏的"动作分段"黑话。Percussion 把它落到 `SkillCast` 三阶段。

- **windup**（前摇 / 蓄力）— 攻击键按下到判定真正生效之间的时间段。
  设计目的：给对手"反应窗口"和"动作可读性"——挥剑必须先抡起来才能砍下。
- **active**（攻击生效期 / 攻击判定窗口）— 攻击 hitbox 实际存在 / 能造成伤害的时间段。
- **recovery**（后摇 / 硬直）— 攻击判定结束到角色能再次行动之间的时间段。
  设计目的：让玩家"挥空一刀有代价"，鼓励出招前选时机。
- **startup frame** / **endlag**（启动帧 / 结束帧）— 同 `windup` / `recovery`，但偏格斗游戏圈黑话，按"帧数"度量（24 帧 startup = 0.4 秒前摇）。
- **cooldown**（冷却时间）— 技能用完到能再次使用之间的等待时间。跟 `recovery` 区别：`recovery` 锁的是**整个角色**，`cooldown` 锁的是**单个技能**（可以同时用别的）。
- **cast time**（吟唱时间）— 法术类技能的 `windup`，常带读条 UI。
- **channel**（引导）— 技能"持续生效"的时间段（如持续光束、读秒治疗）。可以被打断。
- **hitstun**（受击硬直 / 命中僵直）— 被打中后短暂无法操作的时间。让连段成立。
- **stagger**（破防 / 踉跄）— 比 hitstun 更长的硬直，常配合"霸体值"被打空触发。
- **iframes** / **invincibility frames**（无敌帧）— 翻滚 / 突进期间临时无敌的那几帧。受击判定不响应。
- **cancel**（取消 / 接技）— 在某个动作的特定窗口里强制中断它进入下一个动作。"普攻取消接闪现" = 普攻 recovery 还没走完就触发闪现。
- **combo**（连段）— 一串首尾相接、对手中间无法插入操作的连击。靠 hitstun 链起来。
- **telegraph**（预兆 / 起手提示）— Boss 大招前的明显视觉信号（地面光圈、咆哮），让玩家"看得见才躲得开"。
- **super armor** / **poise**（霸体 / 韧性）— 抗 hitstun 的属性。霸体期间被打也不中断动作。

---

## 2. 命中判定 / 攻击形态

- **hitbox**（攻击判定盒）— 攻击方挂的"我能打到这块区域"的几何体。
- **hurtbox**（受击判定盒）— 防守方挂的"我能被打到的部位"的几何体。
  hitbox ∩ hurtbox = 命中。两者**分离**是动作游戏的核心抽象——同一只怪可以有头 / 身 / 腿三个 hurtbox 算不同伤害。
- **AOE** / **Area of Effect**（范围攻击）— 不针对单点的群伤攻击。
- **cone** / **sector**（圆锥 / 扇形）— AOE 的常见形状之一，扇形角度 + 半径定义。
  注：3D 里"cone"是圆锥体，2D / top-down 里习惯叫"sector"（数学的扇形）。
- **melee**（近战）— 短距离 / 接触攻击。
- **ranged**（远程）— 不接触目标的攻击（弓箭、法术、枪）。
- **reach**（攻击距离 / 伸臂）— 近战武器"够得到多远"。
- **projectile**（投射物 / 弹）— 发射后离开攻击者独立飞行的攻击体（箭、子弹、火球）。
- **homing**（追踪 / 制导）— projectile 飞行中持续修正方向追目标。
- **pierce** / **piercing**（穿透）— projectile 命中后**不消失**，继续飞行，能打到下一个目标。
- **cleave**（横扫 / 群伤）— 一次近战攻击命中多个目标（vs 单体）。
- **knockback**（击退）— 命中时把目标推开。区别于 hitstun（只硬直不移动）。
- **juggle**（浮空连段）— 把目标打浮空后在落地前继续命中。
- **friendly fire**（友军伤害）— 是否会误伤同阵营单位。
- **faction**（阵营）— 用来判断"敌我"的身份标签。同 faction 之间默认不打。

---

## 3. 数值 / RPG 战斗

- **modifier**（修正 / 改值器）— 一段可加可减的伤害 / 属性变换（×1.5、+10、暴击 ×2）。
  modifier 串成"流水线"是 ARPG 数值系统的标准抽象。Percussion 的 `DamageModifier` 就这个意思。
- **multiplier**（倍率）— 乘法修正的简称（"力量加 50% 伤害"= multiplier 1.5）。
- **crit** / **critical hit**（暴击）— 概率触发的"伤害翻倍"事件。
- **crit chance**（暴击率）— 触发暴击的概率，一般 0.0–1.0。
- **crit multiplier**（暴击倍率）— 触发后的伤害倍数，常见 ×1.5 / ×2。
- **DPS** / **Damage Per Second**（每秒伤害）— 武器 / build 的核心比较指标。
- **DoT** / **Damage over Time**（持续伤害）— 中毒 / 燃烧 / 流血这种"挂上去之后每秒掉血"的 debuff。
- **HoT** / **Heal over Time**（持续治疗）— DoT 的反面，持续回血的 buff。
- **buff**（增益 / 加成）— 临时正面状态。可叠加 / 可被驱散。
- **debuff**（减益 / 负面状态）— 临时负面状态，DoT 是 debuff 的一种。
- **CC** / **Crowd Control**（控制效果）— 眩晕 / 冰冻 / 缴械 / 沉默这一类**限制行动**的 debuff 总称。
- **proc**（触发 —— procedure 缩写）— 概率事件实际发生的那一刻。"50% 暴击 procced" = 这次确实暴了。
- **tick**（一次结算）— 持续效果的一次发生。"燃烧每秒 tick 一次扣 5 血"。
- **roll**（掷骰 / 概率结算）— 一次随机判定。"crit roll" = 算这次是否暴击。
- **lifesteal** / **vampirism**（吸血 / 嗜血）— 造成的伤害按比例回血给攻击者。
- **on-hit effect**（命中时触发）— 命中那一刻发生的衍生效果（点燃、击退、吸血），跟"造成伤害"逻辑分离。

---

## 4. AI / 行为

Percussion 当前 AI 很简单，但这些是行业通用词，看英文资料会遇到。

- **aggro** / **hate** / **threat**（仇恨）— AI"想打谁"的优先级。WoW 系 MMO 出来的词。
- **threat table**（仇恨表）— 每个敌人维护"队伍里谁的仇恨值最高"的列表。
- **pathfinding**（寻路）— 从 A 到 B 找路径。最常见算法是 A*。
- **A\***（A-star）— 启发式寻路算法，几乎是默认选择。
- **navmesh**（导航网格 / 导航网面）— 把可走区域预处理成多边形网，寻路在网上跑。比 grid 寻路灵活。
- **steering**（转向 / 局部避障）— 微观移动决策：怎么转向、怎么绕开同伴。pathfinding 算"宏观去哪"，steering 算"具体每帧怎么走"。
- **LOS** / **line of sight**（视线 / 视野）— "我能不能看见目标"的判定。常用 raycast。
- **leash**（拉绳）— 怪物离出生点超过一定距离就回去的机制，防止被引到地图外。
- **flocking** / **boids**（群体行为）— 让一群单位自然成群（分离 / 对齐 / 内聚三规则）。
- **FSM** / **state machine**（有限状态机）— Idle → Chase → Attack → Flee 的标准 AI 框架。
- **behavior tree**（行为树）— 比 FSM 更结构化的 AI 决策框架。节点有 sequence / selector / decorator。
- **utility AI**（效用 AI）— 每个候选行为打分，选分最高的。比 FSM / BT 更动态。
- **patrol** / **chase** / **flee**（巡逻 / 追击 / 逃跑）— AI 状态机里常见的几个 state 名。

---

## 5. 物理 / 碰撞（通用概念，跟 avian 解耦）

- **kinematic**（运动学物体）— 不受力影响、由代码每帧直接设位置的物体。角色控制器几乎都是 kinematic。
- **dynamic**（动力学物体）— 受重力 / 推力 / 摩擦影响、由物理引擎积分更新的物体。物理箱子 / 布娃娃。
- **static**（静态物体）— 永远不动的物体。地面、墙。引擎可以做特别优化。
- **sweep**（扫掠检测）— 模拟物体沿一段轨迹移动，检测沿途碰到什么。比"先移动再检测"更准。
- **slide**（沿障碍滑动）— 撞墙后不停下而是沿墙滑。3D 平台跳跃和俯视移动的标配。
- **sweep-and-slide**（扫掠 + 滑动）— kinematic 角色移动的标准算法：sweep 找碰撞 → 修正速度 → 沿切线滑 → 再 sweep……
- **broad-phase**（粗筛阶段）— 物理引擎第一步：用 AABB 等便宜方法快速排除"绝对不可能碰"的物体对。
- **narrow-phase**（精算阶段）— 物理引擎第二步：对粗筛留下的对，真的算两个 shape 是否 / 在哪里相交。
- **sensor** / **trigger**（传感器 / 触发器）— 形状参与碰撞检测但**不产生物理推力**，只发"重叠了"事件。门、陷阱、领域光环常用。
- **CCD** / **continuous collision detection**（连续碰撞检测）— 防止高速物体在两帧之间"穿透"薄物体的机制。子弹和墙之间必须有 CCD。
- **AABB** / **axis-aligned bounding box**（轴对齐包围盒）— 边平行于坐标轴的长方体。最便宜的近似形状。
- **capsule**（胶囊体）— 圆柱两端加半球。最常用的角色 body 形状，因为底部圆角可以自然滑过台阶 / 斜面。
- **cuboid** / **box**（长方体）— 直角长方体碰撞形状。
- **friction**（摩擦系数）— 物体间相对滑动的阻力。
- **restitution**（弹性系数）— 碰撞后保留多少动能，1.0 = 完全弹回，0.0 = 不弹。
- **raycast**（射线检测）— 从一点沿一方向射"无限细"的射线，找最近碰到什么。视线 / 子弹判定常用。

---

## 6. 3D / 渲染

- **billboard**（告示板 / 卡牌）— 永远朝向相机的平面 sprite。Percussion 的核心视觉技术（饥荒同款）。
- **sprite**（精灵 / 角色图）— 用 2D 图片表示的角色 / 物体。在 3D 游戏里通常用 billboard quad 承载。
- **sprite atlas** / **sprite sheet**（精灵图集 / 雪碧图）— 把许多小 sprite 拼到一张大图里，节省 draw call 和切换 texture 开销。
- **9-slice** / **nine-patch**（九宫格切图）— UI 边框拉伸不变形的标准做法：四角不缩放、四边拉一个方向、中间拉两个方向。
- **quad**（四边形）— 2 个三角形拼出的矩形 mesh，渲染 sprite 用的最常见 mesh。
- **draw call**（绘制调用）— GPU 一次"画这一批东西"的命令。draw call 多 = CPU→GPU 通信开销大 = 帧率掉。
- **vsync** / **vertical sync**（垂直同步）— 显卡 present 时机锁到显示器刷新率，避免画面撕裂。
- **frame pacing**（帧节奏）— 每帧间隔是否均匀。即使平均 60fps，间隔忽 10ms 忽 30ms 也会觉得卡。
- **FOV** / **field of view**（视场角）— 相机看出去的张角。FOV 越大画面越广 / 物体越远小。
- **frustum culling**（视锥剔除）— 不在相机视锥里的物体跳过渲染。引擎自动做。
- **z-fighting** / **depth fighting**（深度冲突 / Z-冲突）— 两个面深度几乎相同，每帧抖动闪烁。Z-buffer 精度不够。
- **alpha test** / **alpha blend**（alpha 测试 / alpha 混合）— 处理 sprite 透明边的两种方式。test = 像素要么显示要么不显示（硬边、不打乱深度）；blend = 半透明混色（软边、需要按距离排序）。
- **PBR** / **physically based rendering**（基于物理的渲染）— 用"金属度 + 粗糙度 + 法线"的标准材质模型。Bevy `StandardMaterial` 就是 PBR。
- **shader**（着色器）— 在 GPU 上跑的小程序，决定像素 / 顶点最终颜色。

---

## 7. ECS / 调度模式

不是 Bevy API 文档，是 ECS / 数据驱动设计这个范式的通用概念。

- **ECS** / **Entity Component System**（实体 - 组件 - 系统）— 把"对象"拆成 entity（ID）+ component（数据）+ system（逻辑）三层的设计范式。跟 OOP 的对应物：entity ≈ 引用，component ≈ 字段，system ≈ 方法，但**横向切片**而非纵向继承。
- **marker component** / **tag component**（标记组件）— **没有字段**的 component，纯靠"挂没挂上"表达布尔状态。Bevy 里 `Player` / `Dead` / `IsGround` 都是这种。
- **query** / **query filter**（查询 / 查询过滤）— 在 ECS 里筛选 entity 的语法。`With<Player>` = "必须挂了 Player"，`Without<Dead>` = "必须没挂 Dead"。
- **system set** / **set barrier**（系统集 / 屏障）— 把多个 system 归到一个"集合"，集合之间靠 `.before()` / `.after()` 排顺序。比逐个 system 排序紧凑得多。
- **happens-before**（发生在……之前）— 调度术语：A happens-before B = 保证 A 跑完才跑 B。也是内存模型常用语。
- **observer**（观察者）— 响应 component 增删 / 事件触发的回调式 system。Bevy 0.16+ 引入。区别于普通 system：observer 是**事件驱动**，普通 system 是**每帧轮询**。
- **message** vs **event**（消息 vs 事件）— Bevy 0.18 把 `Event` / `EventReader` / `EventWriter` 改名为 `Message` / `MessageReader` / `MessageWriter`，因为它们行为更像"广播消息队列"而非"一次性触发事件"。新代码用 message。
- **despawn**（销毁 entity）— ECS 里"删 entity"的术语，对应 spawn。Bevy 里 despawn 通过 `Commands` 入队，**下一次 flush 才真的执行**——所以"刚 despawn 的 entity 当帧 query 可能还能看到"。
- **bundle**（组件包）— 一组 component 的预定义集合，spawn 时一次性挂上去。Bevy 0.15+ 的 `#[require(...)]` 是更现代的替代。
- **flush** / **command buffer flush**（命令冲洗）— Bevy `Commands` 上的 spawn / despawn / insert 不是立即生效的，攒在缓冲里，到调度的同步点才"一次性 apply"。
- **happens-before barrier** / **chain**（顺序屏障 / 链式）— `chain()` 把一组 system 强制串行：A → B → C，B 看得到 A 的结果。
- **defer**（延迟）— 把"立刻做"推到"稍后做"。`Commands` 的本质就是 defer：写代码读着像同步，实际异步。

---

## 8. 项目内反复出现但不严格属于上述分类

少量遗漏词的杂项区。

- **plugin**（插件）— Bevy 把"一组相关 system + component + resource"打包的单位。注意：Bevy plugin **跟编辑器 plugin / 浏览器 plugin 是同名异概念**，不要混淆。
- **prelude**（预导入模块）— `use foo::prelude::*` 一次性引入这个 crate 的高频类型。Rust 生态约定俗成的命名。
- **handle**（资源句柄）— 对一份 asset 的引用 / ID，不持有数据本体。Bevy 的 `Handle<Image>` 本质是个引用计数 ID。
- **schedule**（调度）— Bevy 里"这一组 system 在什么时机跑"的时间表。`Update` / `PostUpdate` / `Startup` 是常见的几个。
- **deterministic**（确定性 / 可重放）— 同输入必同输出。多人同步 / replay 系统的硬约束。要求所有逻辑不读 wall clock、不读非种子化随机数。
- **wall clock**（墙上时钟 / 物理时间）— 跟"游戏内时间"对立的"现实时间"。物理引擎用墙上时钟会让暂停 / 慢放出问题，所以引擎里都是"按 `Time::delta` 推进"。

---

## 编辑约定

写新条目时：

1. **只收业界通用词**——"以后我搜英文文档会再遇到的"。项目自己起的名字不进。
2. **每条最多 3 行**。一行定义、一行"为什么有这个词 / 项目里在哪用"。超了说明在写教程不是词典。
3. **不教 API**——`Component` / `Query` 这种 Bevy 类型名留给 docs.rs。
4. **缩写写全**——`DPS` 后面写出 `Damage Per Second`，方便检索。
