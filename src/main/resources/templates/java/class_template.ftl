package ${packageName};

public class ${className} {

<#-- Generate fields -->
<#list fields as field>
    private ${field.type} ${field.name};
</#list>

<#-- Generate getter and setter methods -->
<#list fields as field>
    public ${field.type} get${field.name?cap_first}() {
    return ${field.name};
    }

    public void set${field.name?cap_first}(${field.type} ${field.name}) {
    this.${field.name} = ${field.name};
    }
</#list>

}
