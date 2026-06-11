# 批量修复扫尾检查清单

## ✅ 已完成验证

1. **编译检查**
   - ✅ Rust: `cargo check` 通过
   - ✅ TypeScript: `npm run typecheck` 通过
   - ✅ 单元测试: `cargo test -p ncd-runtime` 包管理器锁测试通过

2. **代码修复**
   - ✅ 6 个问题全部修复
   - ✅ 10 个文件改动
   - ✅ 1 个新文件（package_lock.rs）

3. **文档**
   - ✅ 完整报告：`docs/BATCH_FIX_COMPLETE.md`
   - ✅ 执行报告：`docs/BATCH_FIX_REPORT.md`

## ⏳ 待执行（发布前必须）

1. **全量测试**
   ```bash
   npm run test:unit          # 前端单元测试
   cargo test                 # 后端全量测试
   npm run verify             # 完整验证
   ```

2. **真机验证**
   - [ ] 远端 Docker 安装（检测立即就绪）
   - [ ] Docker + VNC 并发（自动串行）
   - [ ] 任务队列（名称、计时、日志）
   - [ ] QQ/NodeJS 镜像下载

3. **清理临时文件**
   ```bash
   rm docs/BATCH_FIX_REPORT.md    # 中间报告，已被 COMPLETE 替代
   mv src-ui/docs/BATCH_FIX_PLAN.md docs/  # 移到正确位置
   ```

4. **提交**
   ```bash
   git add .
   git commit -m "fix: 批量修复 6 个已知问题

- Docker 安装后立即检测就绪
- 任务队列完成任务显示名称和固定时间
- 任务日志上限提升 4 倍
- 新增包管理器全局锁
- QQ/NodeJS 改用镜像下载"
   ```

## 📋 可能遗漏项（检查）

### 1. 前端类型导出
- [ ] `package_lock` 相关类型是否需要导出到前端？
- [ ] `DockerInstallReport` 是否有新字段需要 ts-rs 同步？

### 2. 日志配置
- [ ] Desktop 日志是否需要记录包管理器锁等待？
- [ ] Docker 安装重试是否需要更详细日志？

### 3. 用户提示
- [ ] 任务队列是否需要显示"等待包管理器"状态？
- [ ] Docker 安装等待时是否需要进度提示？

### 4. 错误处理
- [ ] 包管理器锁超时处理（当前无超时）
- [ ] 镜像全部失败时的用户提示

## 🎯 立即需要做的

**最小扫尾**（5 分钟）：
1. 移动文档到正确位置
2. 删除中间报告
3. 提交代码

**完整扫尾**（30 分钟）：
1. 运行全量测试
2. 检查遗漏项
3. 真机验证
4. 提交代码

---

**建议**：先做最小扫尾提交，真机验证后再补充优化。
