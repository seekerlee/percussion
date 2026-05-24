# `motion/` —— 运动学模块（规划中，尚未填充）

## 这个模块要解决什么问题

任何"会在世界里移动"的 entity 都要回答**每帧往哪移动多少** —— 直线、
抛物线、追踪敌人、贝塞尔曲线 …… 这套运动学**跟"是不是参与命中"无关**。
未来 `Projectile`（路径命中弹）和 `Missile`（视觉锁定弹）都会需要同一
套运动学组件。

## 为什么现在是空的

按 YAGNI，目前唯一的运动用户是 [`Projectile`](../projectile.rs) 的直线
匀速弹，实现就地在 [`projectile/linear.rs`](../projectile/linear.rs)，
没必要抽离。

**第二种 motion 用户出现时**（最可能是 `Missile`：WC3 弓箭手 / 火枪手
类视觉锁定弹，需要抛物线 / 追踪 / 直线），此目录开始填充：

```text
motion/
├── mod.rs
├── linear.rs       直线匀速 —— 从 projectile/linear.rs 搬过来，
│                   要拆掉 projectile-specific 的"撞墙就 despawn projectile"
├── parabolic.rs    抛物线（初速 + 重力 / 飞行时间）
└── homing.rs       追踪（每帧向 target 转向）
```

## 设计原则（抽离时要守住）

1. **motion ↔ 命中判定正交**：motion 模块**只写 Transform**，不发命中
   消息、不 despawn entity。命中 / 销毁是 motion 用户（Projectile /
   Missile）自己的事，由它们订阅"撞墙信号"或自查位置完成。
2. **组件能力风格**：`LinearMotion` / `ParabolicMotion` / `HomingMotion`
   是**能力 component**；entity 挂哪个就动哪个，互不依赖。
3. **不依赖 avian**：motion 只算 Transform。"撞墙" / "撞 unit" 由用户
   自己决定（projectile 走 shape_cast，missile 一般不需要撞墙）。

## 跟现有代码的关系

- [`projectile/linear.rs`](../projectile/linear.rs)：当前**混了两件事** ——
  纯运动（推 Transform）+ projectile-specific（撞墙 shape_cast 后
  despawn projectile）。抽离时要把后者留在 projectile 模块、订阅 motion
  发出的"撞墙"信号。
- [`unit/movement.rs`](../unit/movement.rs)：unit 的 sweep-and-slide
  移动**跟 motion 模块不同范畴** —— 那个用 avian shape_cast 做地形碰
  撞响应、有 `OnGround` / `MoveVelocity` 等 unit-specific 状态。两者都
  写 Transform，但不复用代码。

## 跟 Projectile / Missile 的关系（路线图）

| entity 类型 | 参与命中 | motion 组合 |
|---|---|---|
| `Projectile`（穿刺箭 / 火球 / 冲击波） | ✅ 路径命中 | Linear / Parabolic / Homing |
| `Missile`（WC3 弓箭手 / 火枪手视觉弹，未来加） | ❌ 仅视觉 | Linear / Parabolic / Homing |

Missile 跟配套的 [`Strike::SingleTarget`](../unit/strike.rs) 共同表达"已
锁定的远程攻击"：Strike 决定打没打中、Missile 只负责视觉飞行 + 飞到
target 位置后 despawn。
