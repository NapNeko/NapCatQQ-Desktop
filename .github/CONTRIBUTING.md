## 目录
1. [命名规范](#命名规范)
    - [模块和包命名](#模块和包命名)
    - [函数命名规范](#函数命名规范)
        - [一般函数](#一般函数)
        - [设置器函数（Setter）](#设置器函数setter)
        - [获取器函数（Getter）](#获取器函数getter)
        - [带有副作用的函数](#带有副作用的函数)
    - [类名命名规范](#类名命名规范)
        - [类名命名](#类名命名)
        - [获取类属性的方法（Getter）](#获取类属性的方法getter)
        - [设置类属性的方法（Setter）](#设置类属性的方法setter)
        - [动作或行为类方法](#动作或行为类方法)
        - [查询方法](#查询方法)
        - [判断方法](#判断方法)
        - [删除方法](#删除方法)
        - [异常处理相关方法](#异常处理相关方法)
        - [执行操作类方法](#执行操作类方法)
    - [变量命名规范](#变量命名)
2. [文件结构](#文件结构)
3. [代码格式](#代码格式)
4. [文档和注释](#文档和注释)
5. [版本控制](#版本控制)

## 命名规范

### 模块和包命名
- 使用 **小写字母**，如果有多个单词，使用下划线分隔。
  - 示例：`my_module.py`, `data_processing/`

### 函数命名规范

#### 一般函数
- 使用 **camelCase**。
- 动词 + 名词，描述函数的行为。
  - 示例：`calculateTotal()`, `getUserInfo()`, `fetchData()`

#### 设置器函数（Setter）
- 对于设置数据的函数，使用 `set` 前缀。
  - 示例：`setUserName()`, `setItemPrice()`

#### 获取器函数（Getter）
- 对于获取数据的函数，使用 `get` 前缀。
  - 示例：`getUser()`, `getItemById()`

#### 带有副作用的函数
- 对于会修改状态或引发副作用的函数，使用动词形式表示。
  - 示例：`saveData()`, `deleteItem()`, `updateUser()`

### 类名命名规范

#### 类名命名
- 使用 **PascalCase**（大驼峰命名法），每个单词的首字母大写
  - 示例：`UserProfile`, `OrderDetails`, `ProductService`

#### 类属性命名
- UI 控件的属性名使用 **camelCase**
  - 示例：`userName`，`totalAmount`，`isActive`
- 其他属性使用 **snake_case**
  - 示例：`user_name`，`total_amount`，`is_active`


#### 获取类属性的方法（Getter）
- 对于获取某个属性的值，方法名以 `get` 开头，后跟属性名。
  - 示例：`getUserName()`, `getOrderDetails()`, `getProductList()`

#### 设置类属性的方法（Setter）
- 对于设置某个属性的值，方法名以 `set` 开头，后跟属性名。
  - 示例：`setUserName()`, `setOrderStatus()`, `setPaymentMethod()`

#### 动作或行为类方法
- 对于执行特定操作的类方法，通常以动词开头，描述该方法的行为。
  - 示例：`saveOrder()`, `updateUserProfile()`, `processPayment()`

#### 查询方法
- 对于查询某些信息的方法，可以使用 `find`, `get` 等动词开头。
  - 示例：`findUserById()`, `getOrderById()`, `findProductByName()`

#### 判断方法
- 对于判断某个条件的类方法，以 `is`, `has`, `can` 开头。
  - 示例：`isUserActive()`, `hasPermission()`, `canPlaceOrder()`

#### 删除方法
- 对于删除操作，方法名通常以 `delete` 开头。
  - 示例：`deleteUser()`, `deleteOrder()`, `removeItem()`

#### 异常处理相关方法
- 如果方法专注于错误处理或异常捕获，命名时可以以 `handle` 开头。
  - 示例：`handleError()`, `handleTimeout()`, `handleInvalidInput()`

#### 执行操作类方法
- 对于执行某项操作的方法，可以使用 `execute`, `run` 等动词。
  - 示例：`executePayment()`, `runTask()`, `startProcess()`


### 变量命名
- 使用 **snake_case**，且应具有描述性。
  - 示例：`user_name`, `total_amount`, `is_active`

## 文件结构
```
my_project/
│
├── src/                  # 源代码目录
│   ├── __init__.py      # 包标识
│   ├── main.py          # 主程序文件
│   └── module/          # 子模块
│       ├── __init__.py  # 包标识
│       └── helper.py    # 辅助函数
│
├── tests/                # 测试目录
│   ├── __init__.py      # 包标识
│   └── test_main.py     # 主程序测试文件
│
├── requirements.txt      # 项目依赖
├── README.md             # 项目说明
└── .gitignore            # Git 忽略文件
```

## 代码格式
- 遵循 PEP 8 风格指南。
- 每行最大字符数应为 79 个字符。
- 使用 4 个空格进行缩进，不要使用制表符（Tab）。
- 在函数和类之间保留两行空行，在类中的方法之间保留一行空行

## 文档和注释
- 使用文档字符串（docstring）为模块、类和函数提供描述
- 文档字符串应使用三重引号，并包括参数和返回值的说明
  - 示例：
  ```python
  def get_user_info(user_id):
      """
      获取用户信息。

      :param user_id: 用户的唯一标识符
      :return: 用户信息字典
      """
  ```


## 版本控制
- 使用 Git 进行版本控制，遵循[Git 分支规范](https://www.atlassian.com/git/tutorials/comparing/git-branching-strategy)：
  - 主分支为 `main`。
  - 功能性开发使用 `feature/` 前缀，例如：`feature/add-user-authentication`
  - 修复和维护使用 `bugfix/` 前缀，例如：`bugfix/fix-typo`
  - 