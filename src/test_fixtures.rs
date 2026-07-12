//! Inline XSD test fixtures and registry builder.
//!
//! These constants provide self-contained XSD schemas that exercise the NIEM
//! patterns (extension, substitution groups, augmentation points, wrappers),
//! so integration tests do not depend on external files.

use std::path::{Path, PathBuf};

use crate::readers::xsd::model::XsdSchema;
use crate::readers::xsd::parser::parse_schema;
use crate::readers::xsd::resolver::{build_type_registry, TypeRegistry};

// ---------------------------------------------------------------------------
// 1. STRUCTURES_XSD
// ---------------------------------------------------------------------------

pub const STRUCTURES_XSD: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<xs:schema
    targetNamespace="http://release.niem.gov/niem/structures/5.0/"
    xmlns:structures="http://release.niem.gov/niem/structures/5.0/"
    xmlns:xs="http://www.w3.org/2001/XMLSchema">

  <xs:attribute name="id" type="xs:ID"/>
  <xs:attribute name="ref" type="xs:IDREF"/>
  <xs:attribute name="uri" type="xs:anyURI"/>
  <xs:attribute name="metadata" type="xs:IDREFS"/>
  <xs:attribute name="sequenceID" type="xs:positiveInteger"/>
  <xs:attribute name="relationshipMetadata" type="xs:IDREFS"/>

  <xs:attributeGroup name="SimpleObjectAttributeGroup">
    <xs:attribute ref="structures:id"/>
    <xs:attribute ref="structures:ref"/>
    <xs:attribute ref="structures:uri"/>
    <xs:attribute ref="structures:metadata"/>
    <xs:attribute ref="structures:sequenceID"/>
    <xs:attribute ref="structures:relationshipMetadata"/>
    <xs:anyAttribute namespace="urn:us:gov:ic:ism urn:us:gov:ic:ntk" processContents="lax"/>
  </xs:attributeGroup>

  <xs:complexType name="ObjectType" abstract="true">
    <xs:sequence>
      <xs:element ref="structures:ObjectAugmentationPoint" minOccurs="0" maxOccurs="unbounded"/>
    </xs:sequence>
    <xs:anyAttribute namespace="urn:us:gov:ic:ism urn:us:gov:ic:ntk" processContents="lax"/>
  </xs:complexType>

  <xs:complexType name="AssociationType" abstract="true">
    <xs:sequence>
      <xs:element ref="structures:AssociationAugmentationPoint" minOccurs="0" maxOccurs="unbounded"/>
    </xs:sequence>
  </xs:complexType>

  <xs:element name="ObjectAugmentationPoint" abstract="true"/>
  <xs:element name="AssociationAugmentationPoint" abstract="true"/>

</xs:schema>
"#;

// ---------------------------------------------------------------------------
// 2. NIEM_XS_XSD
// ---------------------------------------------------------------------------

pub const NIEM_XS_XSD: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<xs:schema
    targetNamespace="http://release.niem.gov/niem/proxy/niem-xs/5.0/"
    xmlns:niem-xs="http://release.niem.gov/niem/proxy/niem-xs/5.0/"
    xmlns:structures="http://release.niem.gov/niem/structures/5.0/"
    xmlns:xs="http://www.w3.org/2001/XMLSchema">

  <xs:import namespace="http://release.niem.gov/niem/structures/5.0/"/>

  <xs:complexType name="string">
    <xs:simpleContent>
      <xs:extension base="xs:string">
        <xs:attributeGroup ref="structures:SimpleObjectAttributeGroup"/>
      </xs:extension>
    </xs:simpleContent>
  </xs:complexType>

  <xs:complexType name="boolean">
    <xs:simpleContent>
      <xs:extension base="xs:boolean">
        <xs:attributeGroup ref="structures:SimpleObjectAttributeGroup"/>
      </xs:extension>
    </xs:simpleContent>
  </xs:complexType>

  <xs:complexType name="double">
    <xs:simpleContent>
      <xs:extension base="xs:double">
        <xs:attributeGroup ref="structures:SimpleObjectAttributeGroup"/>
      </xs:extension>
    </xs:simpleContent>
  </xs:complexType>

  <xs:complexType name="float">
    <xs:simpleContent>
      <xs:extension base="xs:float">
        <xs:attributeGroup ref="structures:SimpleObjectAttributeGroup"/>
      </xs:extension>
    </xs:simpleContent>
  </xs:complexType>

  <xs:complexType name="integer">
    <xs:simpleContent>
      <xs:extension base="xs:integer">
        <xs:attributeGroup ref="structures:SimpleObjectAttributeGroup"/>
      </xs:extension>
    </xs:simpleContent>
  </xs:complexType>

  <xs:complexType name="int">
    <xs:simpleContent>
      <xs:extension base="xs:int">
        <xs:attributeGroup ref="structures:SimpleObjectAttributeGroup"/>
      </xs:extension>
    </xs:simpleContent>
  </xs:complexType>

  <xs:complexType name="long">
    <xs:simpleContent>
      <xs:extension base="xs:long">
        <xs:attributeGroup ref="structures:SimpleObjectAttributeGroup"/>
      </xs:extension>
    </xs:simpleContent>
  </xs:complexType>

  <xs:complexType name="nonNegativeInteger">
    <xs:simpleContent>
      <xs:extension base="xs:nonNegativeInteger">
        <xs:attributeGroup ref="structures:SimpleObjectAttributeGroup"/>
      </xs:extension>
    </xs:simpleContent>
  </xs:complexType>

  <xs:complexType name="positiveInteger">
    <xs:simpleContent>
      <xs:extension base="xs:positiveInteger">
        <xs:attributeGroup ref="structures:SimpleObjectAttributeGroup"/>
      </xs:extension>
    </xs:simpleContent>
  </xs:complexType>

  <xs:complexType name="unsignedInt">
    <xs:simpleContent>
      <xs:extension base="xs:unsignedInt">
        <xs:attributeGroup ref="structures:SimpleObjectAttributeGroup"/>
      </xs:extension>
    </xs:simpleContent>
  </xs:complexType>

  <xs:complexType name="decimal">
    <xs:simpleContent>
      <xs:extension base="xs:decimal">
        <xs:attributeGroup ref="structures:SimpleObjectAttributeGroup"/>
      </xs:extension>
    </xs:simpleContent>
  </xs:complexType>

  <xs:complexType name="dateTime">
    <xs:simpleContent>
      <xs:extension base="xs:dateTime">
        <xs:attributeGroup ref="structures:SimpleObjectAttributeGroup"/>
      </xs:extension>
    </xs:simpleContent>
  </xs:complexType>

  <xs:complexType name="duration">
    <xs:simpleContent>
      <xs:extension base="xs:duration">
        <xs:attributeGroup ref="structures:SimpleObjectAttributeGroup"/>
      </xs:extension>
    </xs:simpleContent>
  </xs:complexType>

  <xs:complexType name="base64Binary">
    <xs:simpleContent>
      <xs:extension base="xs:base64Binary">
        <xs:attributeGroup ref="structures:SimpleObjectAttributeGroup"/>
      </xs:extension>
    </xs:simpleContent>
  </xs:complexType>

</xs:schema>
"#;

// ---------------------------------------------------------------------------
// 3. CORE_TYPES_XSD
// ---------------------------------------------------------------------------

pub const CORE_TYPES_XSD: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<xs:schema
    targetNamespace="http://example.com/schemas/common-types"
    xmlns:ct="http://example.com/schemas/common-types"
    xmlns:structures="http://release.niem.gov/niem/structures/5.0/"
    xmlns:niem-xs="http://release.niem.gov/niem/proxy/niem-xs/5.0/"
    xmlns:nc="http://release.niem.gov/niem/niem-core/5.0/"
    xmlns:xs="http://www.w3.org/2001/XMLSchema">

  <xs:import namespace="http://release.niem.gov/niem/structures/5.0/"/>
  <xs:import namespace="http://release.niem.gov/niem/proxy/niem-xs/5.0/"/>
  <xs:import namespace="http://release.niem.gov/niem/niem-core/5.0/"/>

  <!-- Simple types -->

  <xs:simpleType name="ConfidenceCodeSimpleType">
    <xs:restriction base="xs:token">
      <xs:enumeration value="UNKNOWN"/>
      <xs:enumeration value="VERY_LOW"/>
      <xs:enumeration value="LOW"/>
      <xs:enumeration value="MODERATE"/>
      <xs:enumeration value="HIGH"/>
      <xs:enumeration value="VERY_HIGH"/>
    </xs:restriction>
  </xs:simpleType>

  <xs:simpleType name="UuidIdentificationIDSimpleType">
    <xs:restriction base="xs:token">
      <xs:pattern value="[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}"/>
    </xs:restriction>
  </xs:simpleType>

  <xs:simpleType name="String64SimpleType">
    <xs:restriction base="xs:token">
      <xs:maxLength value="64"/>
    </xs:restriction>
  </xs:simpleType>

  <!-- Complex types -->

  <xs:complexType name="WGS84LocationType">
    <xs:complexContent>
      <xs:extension base="structures:ObjectType">
        <xs:sequence>
          <xs:element ref="ct:Latitude"/>
          <xs:element ref="ct:Longitude"/>
          <xs:element ref="ct:Altitude"/>
          <xs:element ref="ct:WGS84LocationAugmentationPoint" minOccurs="0" maxOccurs="unbounded"/>
        </xs:sequence>
      </xs:extension>
    </xs:complexContent>
  </xs:complexType>

  <xs:complexType name="CapabilityConfidenceCodeType">
    <xs:simpleContent>
      <xs:extension base="ct:ConfidenceCodeSimpleType">
        <xs:attributeGroup ref="structures:SimpleObjectAttributeGroup"/>
      </xs:extension>
    </xs:simpleContent>
  </xs:complexType>

  <!-- Elements -->

  <xs:element name="Latitude" type="niem-xs:double"/>
  <xs:element name="Longitude" type="niem-xs:double"/>
  <xs:element name="Altitude" type="niem-xs:double"/>
  <xs:element name="WGS84LocationAugmentationPoint" abstract="true"/>
  <xs:element name="IdentificationCategoryAbstract" abstract="true"/>
  <xs:element name="GlobalIdentifierCategoryCode" type="niem-xs:string" substitutionGroup="ct:IdentificationCategoryAbstract"/>
  <xs:element name="ConfidenceCode" type="ct:CapabilityConfidenceCodeType"/>

</xs:schema>
"#;

// ---------------------------------------------------------------------------
// 4. DOMAIN_XSD
// ---------------------------------------------------------------------------

pub const DOMAIN_XSD: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<xs:schema
    targetNamespace="http://example.com/schemas/vehicle"
    xmlns:veh="http://example.com/schemas/vehicle"
    xmlns:structures="http://release.niem.gov/niem/structures/5.0/"
    xmlns:niem-xs="http://release.niem.gov/niem/proxy/niem-xs/5.0/"
    xmlns:ct="http://example.com/schemas/common-types"
    xmlns:cap="http://example.com/schemas/capability"
    xmlns:xs="http://www.w3.org/2001/XMLSchema">

  <xs:import namespace="http://release.niem.gov/niem/structures/5.0/"/>
  <xs:import namespace="http://release.niem.gov/niem/proxy/niem-xs/5.0/"/>
  <xs:import namespace="http://example.com/schemas/common-types"/>
  <xs:import namespace="http://example.com/schemas/capability"/>

  <!-- Complex types -->

  <xs:complexType name="VehicleType">
    <xs:complexContent>
      <xs:extension base="structures:ObjectType">
        <xs:sequence>
          <xs:element ref="veh:Identification"/>
          <xs:element ref="veh:EntityDetails"/>
          <xs:element ref="veh:AuditRecord" minOccurs="0" maxOccurs="unbounded"/>
          <xs:element ref="veh:VehicleAugmentationPoint" minOccurs="0" maxOccurs="unbounded"/>
        </xs:sequence>
      </xs:extension>
    </xs:complexContent>
  </xs:complexType>

  <xs:complexType name="InspectionReportType">
    <xs:complexContent>
      <xs:extension base="structures:ObjectType">
        <xs:sequence>
          <xs:element ref="veh:StatusCodeAbstract"/>
          <xs:element ref="veh:StatusDescription" minOccurs="0"/>
        </xs:sequence>
      </xs:extension>
    </xs:complexContent>
  </xs:complexType>

  <!-- Elements -->

  <xs:element name="Vehicle" type="veh:VehicleType"/>
  <xs:element name="Identification" type="niem-xs:string"/>
  <xs:element name="EntityDetails" type="niem-xs:string"/>
  <xs:element name="AuditRecord" type="niem-xs:string"/>
  <xs:element name="VehicleAugmentationPoint" abstract="true"/>
  <xs:element name="StatusCodeAbstract" abstract="true"/>
  <xs:element name="StatusCode" type="niem-xs:string" substitutionGroup="veh:StatusCodeAbstract"/>
  <xs:element name="StatusDescription" type="niem-xs:string"/>

</xs:schema>
"#;

// ---------------------------------------------------------------------------
// 5. CAPABILITY_XSD
// ---------------------------------------------------------------------------

pub const CAPABILITY_XSD: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<xs:schema
    targetNamespace="http://example.com/schemas/capability"
    xmlns:cap="http://example.com/schemas/capability"
    xmlns:structures="http://release.niem.gov/niem/structures/5.0/"
    xmlns:niem-xs="http://release.niem.gov/niem/proxy/niem-xs/5.0/"
    xmlns:xs="http://www.w3.org/2001/XMLSchema">

  <xs:import namespace="http://release.niem.gov/niem/structures/5.0/"/>
  <xs:import namespace="http://release.niem.gov/niem/proxy/niem-xs/5.0/"/>

  <xs:element name="FeatureAbstract" abstract="true"/>
  <xs:element name="NetworkFeature" type="niem-xs:string" substitutionGroup="cap:FeatureAbstract"/>
  <xs:element name="SensorFeature" type="niem-xs:string" substitutionGroup="cap:FeatureAbstract"/>
  <xs:element name="ActuatorFeature" type="niem-xs:string" substitutionGroup="cap:FeatureAbstract"/>

</xs:schema>
"#;

// ---------------------------------------------------------------------------
// 6. NIEM_CORE_XSD
// ---------------------------------------------------------------------------

pub const NIEM_CORE_XSD: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<xs:schema
    targetNamespace="http://release.niem.gov/niem/niem-core/5.0/"
    xmlns:nc="http://release.niem.gov/niem/niem-core/5.0/"
    xmlns:structures="http://release.niem.gov/niem/structures/5.0/"
    xmlns:xs="http://www.w3.org/2001/XMLSchema">

  <xs:import namespace="http://release.niem.gov/niem/structures/5.0/"/>

  <xs:element name="LocationAugmentationPoint" abstract="true"/>

  <xs:complexType name="LocationType">
    <xs:complexContent>
      <xs:extension base="structures:ObjectType">
        <xs:sequence>
          <xs:element ref="nc:LocationAugmentationPoint" minOccurs="0" maxOccurs="unbounded"/>
        </xs:sequence>
      </xs:extension>
    </xs:complexContent>
  </xs:complexType>

</xs:schema>
"#;

// ---------------------------------------------------------------------------
// 7. MIL_OPS_XSD
// ---------------------------------------------------------------------------

pub const MIL_OPS_XSD: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<xs:schema
    targetNamespace="http://release.niem.gov/niem/domains/militaryOperations/5.1/"
    xmlns:mo="http://release.niem.gov/niem/domains/militaryOperations/5.1/"
    xmlns:structures="http://release.niem.gov/niem/structures/5.0/"
    xmlns:nc="http://release.niem.gov/niem/niem-core/5.0/"
    xmlns:xs="http://www.w3.org/2001/XMLSchema">

  <xs:import namespace="http://release.niem.gov/niem/structures/5.0/"/>
  <xs:import namespace="http://release.niem.gov/niem/niem-core/5.0/"/>

  <xs:element name="LocationAugmentation" type="structures:ObjectType" substitutionGroup="nc:LocationAugmentationPoint"/>

</xs:schema>
"#;

// ---------------------------------------------------------------------------
// 8. EXAMPLE_CORE_XSD
// ---------------------------------------------------------------------------

pub const EXAMPLE_CORE_XSD: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<xs:schema
    targetNamespace="http://example.com/schemas/core"
    xmlns:core="http://example.com/schemas/core"
    xmlns:structures="http://release.niem.gov/niem/structures/5.0/"
    xmlns:niem-xs="http://release.niem.gov/niem/proxy/niem-xs/5.0/"
    xmlns:nc="http://release.niem.gov/niem/niem-core/5.0/"
    xmlns:ct="http://example.com/schemas/common-types"
    xmlns:veh="http://example.com/schemas/vehicle"
    xmlns:cap="http://example.com/schemas/capability"
    xmlns:mo="http://release.niem.gov/niem/domains/militaryOperations/5.1/"
    xmlns:ns1="http://example.com/ns1"
    xmlns:ns2="http://example.com/ns2"
    xmlns:ns3="http://example.com/ns3"
    xmlns:xs="http://www.w3.org/2001/XMLSchema">

  <xs:import namespace="http://release.niem.gov/niem/structures/5.0/"/>
  <xs:import namespace="http://release.niem.gov/niem/proxy/niem-xs/5.0/"/>
  <xs:import namespace="http://release.niem.gov/niem/niem-core/5.0/"/>
  <xs:import namespace="http://example.com/schemas/common-types"/>
  <xs:import namespace="http://example.com/schemas/vehicle"/>
  <xs:import namespace="http://example.com/schemas/capability"/>
  <xs:import namespace="http://release.niem.gov/niem/domains/militaryOperations/5.1/"/>
  <xs:import namespace="http://example.com/ns1"/>
  <xs:import namespace="http://example.com/ns2"/>
  <xs:import namespace="http://example.com/ns3"/>

  <xs:complexType name="CompositeObjectType">
    <xs:complexContent>
      <xs:extension base="structures:ObjectType">
        <xs:sequence>
          <xs:choice>
            <xs:element ref="veh:Vehicle"/>
            <xs:element ref="core:Element2"/>
            <xs:element ref="core:Element3"/>
            <xs:element ref="core:Element4"/>
            <xs:element ref="core:Element5"/>
            <xs:element ref="core:Element6"/>
            <xs:element ref="core:Element7"/>
            <xs:element ref="core:Element8"/>
            <xs:element ref="core:Element9"/>
            <xs:element ref="core:Element10"/>
            <xs:element ref="core:Element11"/>
            <xs:element ref="core:Element12"/>
            <xs:element ref="core:Element13"/>
            <xs:element ref="core:Element14"/>
          </xs:choice>
        </xs:sequence>
      </xs:extension>
    </xs:complexContent>
  </xs:complexType>

  <xs:element name="CompositeObjectAbstract" abstract="true"/>
  <xs:element name="CompositeObject" type="core:CompositeObjectType" substitutionGroup="core:CompositeObjectAbstract"/>

  <xs:element name="Element2" type="niem-xs:string"/>
  <xs:element name="Element3" type="niem-xs:string"/>
  <xs:element name="Element4" type="niem-xs:string"/>
  <xs:element name="Element5" type="niem-xs:string"/>
  <xs:element name="Element6" type="niem-xs:string"/>
  <xs:element name="Element7" type="niem-xs:string"/>
  <xs:element name="Element8" type="niem-xs:string"/>
  <xs:element name="Element9" type="niem-xs:string"/>
  <xs:element name="Element10" type="niem-xs:string"/>
  <xs:element name="Element11" type="niem-xs:string"/>
  <xs:element name="Element12" type="niem-xs:string"/>
  <xs:element name="Element13" type="niem-xs:string"/>
  <xs:element name="Element14" type="niem-xs:string"/>

</xs:schema>
"#;

// ---------------------------------------------------------------------------
// Registry builder
// ---------------------------------------------------------------------------

/// Parse all 8 inline XSD schemas and build a [`TypeRegistry`].
pub fn build_test_registry() -> TypeRegistry {
    let schemas = vec![
        parse_xsd(STRUCTURES_XSD),
        parse_xsd(NIEM_XS_XSD),
        parse_xsd(CORE_TYPES_XSD),
        parse_xsd(DOMAIN_XSD),
        parse_xsd(CAPABILITY_XSD),
        parse_xsd(NIEM_CORE_XSD),
        parse_xsd(MIL_OPS_XSD),
        parse_xsd(EXAMPLE_CORE_XSD),
    ];
    build_type_registry(schemas)
}

fn parse_xsd(xml: &str) -> (XsdSchema, PathBuf) {
    let schema = parse_schema(xml, Path::new("test.xsd")).expect("failed to parse inline XSD");
    (schema, PathBuf::from("test.xsd"))
}
