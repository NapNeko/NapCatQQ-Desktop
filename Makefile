# NapCatQQ Desktop Makefile
# 支持构建 Desktop 和 Daemon

.PHONY: all help build build-desktop build-daemon clean test lint fmt

# 默认目标
all: build

# 帮助信息
help:
	@echo "NapCatQQ Desktop 构建工具"
	@echo ""
	@echo "使用方法:"
	@echo "  make build          - 构建所有组件"
	@echo "  make build-desktop  - 构建 Desktop (Python)"
	@echo "  make build-daemon   - 构建 Daemon (Go)"
	@echo "  make test           - 运行测试"
	@echo "  make lint           - 代码检查"
	@echo "  make fmt            - 代码格式化"
	@echo "  make clean          - 清理构建产物"
	@echo "  make setup          - 安装开发依赖"
	@echo "  make run-desktop    - 运行 Desktop"
	@echo "  make run-daemon     - 运行 Daemon"
	@echo ""

# 构建所有
build: build-desktop build-daemon
	@echo "✅ 构建完成"

# 构建 Desktop (Python)
build-desktop:
	@echo "🔨 构建 Desktop..."
	python -m pip install -e . -q
	@echo "✅ Desktop 构建完成"

# 构建 Daemon (Go)
build-daemon:
	@echo "🔨 构建 Daemon..."
	cd src/daemon && go build -o ../../bin/napcat-daemon ./cmd/daemon
	@echo "✅ Daemon 构建完成"

# 交叉编译 Daemon (所有平台)
build-daemon-all:
	@echo "🔨 交叉编译 Daemon..."
	mkdir -p bin/releases
	# Linux AMD64
	cd src/daemon && GOOS=linux GOARCH=amd64 go build -o ../../bin/releases/napcat-daemon-linux-amd64 ./cmd/daemon
	# Linux ARM64
	cd src/daemon && GOOS=linux GOARCH=arm64 go build -o ../../bin/releases/napcat-daemon-linux-arm64 ./cmd/daemon
	# Linux ARM
	cd src/daemon && GOOS=linux GOARCH=arm go build -o ../../bin/releases/napcat-daemon-linux-arm ./cmd/daemon
	@echo "✅ Daemon 交叉编译完成"

# 运行测试
test: test-python test-go

# Python 测试
test-python:
	@echo "🧪 运行 Python 测试..."
	python -m pytest script/test -v --tb=short

# Go 测试
test-go:
	@echo "🧪 运行 Go 测试..."
	cd src/daemon && go test -v ./...

# 代码检查
lint: lint-python lint-go

# Python 代码检查
lint-python:
	@echo "🔍 检查 Python 代码..."
	python -m ruff check src/desktop
	python -m mypy src/desktop --ignore-missing-imports

# Go 代码检查
lint-go:
	@echo "🔍 检查 Go 代码..."
	cd src/daemon && go vet ./...

# 代码格式化
fmt: fmt-python fmt-go

# Python 代码格式化
fmt-python:
	@echo "📝 格式化 Python 代码..."
	python -m ruff format src/desktop
	python -m isort src/desktop

# Go 代码格式化
fmt-go:
	@echo "📝 格式化 Go 代码..."
	cd src/daemon && gofmt -w .

# 清理构建产物
clean:
	@echo "🧹 清理构建产物..."
	rm -rf bin/ build/ dist/
	find . -type d -name __pycache__ -exec rm -rf {} + 2>/dev/null || true
	find . -type f -name "*.pyc" -delete 2>/dev/null || true
	@echo "✅ 清理完成"

# 安装开发依赖
setup:
	@echo "📦 安装开发依赖..."
	python -m pip install -r requirements.txt
	python -m pip install -e .
	cd src/daemon && go mod download
	@echo "✅ 依赖安装完成"

# 更新依赖
update-deps:
	@echo "📦 更新依赖..."
	python -m pip install --upgrade -r requirements.txt
	cd src/daemon && go get -u ./...
	cd src/daemon && go mod tidy
	@echo "✅ 依赖更新完成"

# 运行 Desktop
run-desktop:
	@echo "🚀 启动 Desktop..."
	python main.py

# 运行 Daemon (开发模式)
run-daemon:
	@echo "🚀 启动 Daemon..."
	cd src/daemon && go run ./cmd/daemon

# 运行 Daemon (带配置文件)
run-daemon-config:
	@echo "🚀 启动 Daemon (带配置)..."
	./bin/napcat-daemon -config ./config/daemon.yaml

# 创建发布包
release: clean build-daemon-all
	@echo "📦 创建发布包..."
	mkdir -p dist
	# 创建版本信息
	git describe --tags --always > dist/VERSION
	# 复制文件
	cp -r bin/releases dist/
	cp -r src/daemon/scripts dist/
	cp docs/general/quick_deploy_guide.md dist/README.md
	@echo "✅ 发布包创建完成: dist/"

# 安装 Daemon 到本地 (测试)
install-daemon-local: build-daemon
	@echo "📥 安装 Daemon 到本地..."
	install -m 755 bin/napcat-daemon /usr/local/bin/napcat-daemon
	install -m 644 src/daemon/scripts/install.sh /usr/local/bin/napcat-daemon-install.sh
	@echo "✅ 本地安装完成"

# Docker 构建 (可选)
docker-build:
	@echo "🐳 构建 Docker 镜像..."
	docker build -t napcat-daemon:latest .

# 健康检查
health-check:
	@echo "🏥 健康检查..."
	@echo "Python 版本:"
	python --version
	@echo "Go 版本:"
	go version
	@echo "检查关键文件..."
	@test -f main.py || echo "❌ main.py 缺失"
	@test -d src/daemon || echo "❌ src/daemon 缺失"
	@test -d src/desktop || echo "❌ src/desktop 缺失"
	@echo "✅ 健康检查完成"
