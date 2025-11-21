# -*- coding: utf-8 -*-
"""测试单例模式"""
# 标准库导入
import sys
from pathlib import Path

# 添加项目路径
sys.path.insert(0, str(Path(__file__).parent.parent.parent))

# 直接导入模块
import importlib.util

spec = importlib.util.spec_from_file_location(
    "singleton",
    Path(__file__).parent.parent.parent / "src" / "core" / "utils" / "singleton.py"
)
singleton_module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(singleton_module)

Singleton = singleton_module.Singleton


class TestSingleton:
    """测试单例模式元类"""

    def test_singleton_creates_one_instance(self):
        """测试单例模式只创建一个实例"""
        
        class TestClass(metaclass=Singleton):
            def __init__(self):
                self.value = 42
        
        instance1 = TestClass()
        instance2 = TestClass()
        
        # 应该返回同一个实例
        assert instance1 is instance2
        assert id(instance1) == id(instance2)

    def test_singleton_shares_state(self):
        """测试单例实例共享状态"""
        
        class Counter(metaclass=Singleton):
            def __init__(self):
                if not hasattr(self, 'count'):
                    self.count = 0
            
            def increment(self):
                self.count += 1
        
        counter1 = Counter()
        counter1.increment()
        counter1.increment()
        
        counter2 = Counter()
        
        # counter2 应该看到 counter1 的修改
        assert counter2.count == 2

    def test_different_classes_different_instances(self):
        """测试不同的单例类有不同的实例"""
        
        class ClassA(metaclass=Singleton):
            pass
        
        class ClassB(metaclass=Singleton):
            pass
        
        a = ClassA()
        b = ClassB()
        
        # 不同的类应该有不同的实例
        assert a is not b
        assert type(a) != type(b)

    def test_singleton_with_init_params_first_wins(self):
        """测试带参数的单例，第一次初始化的参数生效"""
        
        class ConfigClass(metaclass=Singleton):
            def __init__(self, name="default"):
                if not hasattr(self, 'initialized'):
                    self.name = name
                    self.initialized = True
        
        # 第一次创建带参数
        config1 = ConfigClass("first")
        assert config1.name == "first"
        
        # 第二次创建不同参数，应该忽略
        config2 = ConfigClass("second")
        assert config2.name == "first"  # 仍然是 first
        assert config1 is config2
