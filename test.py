# -*- coding: utf-8 -*-
def calculate_social_security_and_housing_fund(base_salary):
    # 养老保险
    pension_unit = 0.16 * base_salary  # 单位部分
    pension_personal = 0.08 * base_salary  # 个人部分

    # 医疗保险
    medical_unit = 0.105 * base_salary  # 单位部分
    medical_personal = 0.02 * base_salary  # 个人部分

    # 失业保险
    unemployment_unit = 0.005 * base_salary  # 单位部分
    unemployment_personal = 0.005 * base_salary  # 个人部分

    # 工伤保险 (假设最低费率 0.16%)
    injury_unit = 0.0016 * base_salary  # 单位部分

    # 住房公积金（按7%）
    housing_fund_unit = 0.07 * base_salary  # 单位部分
    housing_fund_personal = 0.07 * base_salary  # 个人部分

    # 补充住房公积金（按3%）
    supplementary_housing_fund_unit = 0.03 * base_salary  # 单位部分
    supplementary_housing_fund_personal = 0.03 * base_salary  # 个人部分

    # 结果输出
    result = {
        "养老保险": {"单位": pension_unit, "个人": pension_personal},
        "医疗保险": {"单位": medical_unit, "个人": medical_personal},
        "失业保险": {"单位": unemployment_unit, "个人": unemployment_personal},
        "工伤保险": {"单位": injury_unit},
        "住房公积金": {"单位": housing_fund_unit, "个人": housing_fund_personal},
        "补充住房公积金": {"单位": supplementary_housing_fund_unit, "个人": supplementary_housing_fund_personal}
    }

    return result


# 示例：输入基数为 20000
base_salary = 8500
result = calculate_social_security_and_housing_fund(base_salary)

# 输出结果
for category, amounts in result.items():
    print(f"{category}:")
    for part, amount in amounts.items():
        print(f"  {part}: {amount:.2f} 元")
